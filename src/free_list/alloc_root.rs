use super::node::{AllocationMetadata, AllocationSpecs, Node, ALLOCATION_METADATA_LAYOUT_SIZE};
use std::{
    ptr,
    sync::atomic::{AtomicPtr, Ordering},
};

pub(crate) struct AllocatorRoot {
    pub(crate) free_root: Option<AtomicPtr<u8>>,
}

impl AllocatorRoot {
    /// Allocate memory for the given size and alignment parameters, in place of an existing free Node.
    /// If there is enough space left, add a new free Node with the remaining size.
    ///
    /// **Allocation possibilities**:
    /// - | PAD . ALLOC . ALLOC_METADATA . FILL_PAD |
    /// - | PAD . ALLOC . ALLOC_METADATA . FILL_PAD . FREE_NODE |
    ///
    /// **Blocks**:
    /// - PAD: padding to respect the value alignment requirements
    /// - ALLOC: space for the required value to be allocated
    /// - ALLOC_METADATA: struct containing references to allocation paddings
    ///     - Added padding count (PAD size), may be 0
    ///     - Additional padding count (FILL_PAD size), may be 0
    /// - FILL_PAD: additional padding after the allocated block to fill size up to a Node space
    /// (this is mandatory for deallocation process: must have enough space to allocate a free Node in place of this)
    /// - FREE_NODE: optional free Node instance if there is enough size to place it
    pub(crate) unsafe fn split_alloc(
        &mut self,
        previous: Option<Node>,
        current: Node,
        alloc_specs: AllocationSpecs,
    ) -> *mut u8 {
        let is_root: bool;
        let mut prev_node = if let Some(prev) = previous {
            is_root = false;
            prev
        } else {
            // Dummy node
            is_root = true;
            Node {
                next_ptr: Some(self.free_root.as_mut().unwrap().load(Ordering::Acquire)),
                size: 0,
            }
        };

        let new_node = if alloc_specs.remaining_size != 0 {
            Some(Node {
                next_ptr: None, // Will be set later in the function
                size: alloc_specs.remaining_size,
            })
        } else {
            None
        };

        // calculate allocation ptr (current block start + padding)
        let alloc_ptr = prev_node
            .next_ptr
            .unwrap()
            .cast_mut()
            .add(alloc_specs.padding);

        // Write allocation metadata after value
        let mut ptr_cursor = alloc_ptr.add(alloc_specs.size);
        let metadata = AllocationMetadata {
            align_padding: alloc_specs.padding,
            fill_padding: alloc_specs.fill_padding,
        };
        ptr::write(ptr_cursor as *mut AllocationMetadata, metadata);

        // Add free node
        if let Some(mut node) = new_node {
            // Split the area into allocated and free
            ptr_cursor = ptr_cursor.add(ALLOCATION_METADATA_LAYOUT_SIZE + alloc_specs.fill_padding);
            node.next_ptr = current.next_ptr;
            ptr::write(ptr_cursor as *mut Node, node); // Write Node

            prev_node.next_ptr = Some(ptr_cursor as *const u8);
        } else {
            // No remaining size, simply remove the node
            prev_node.next_ptr = current.next_ptr;
        }

        // Additional work if root node
        if is_root {
            self.free_root = prev_node
                .next_ptr
                .map(|next_ptr| AtomicPtr::new(next_ptr as *mut u8))
        }

        alloc_ptr
    }

    /// Create a new free block Node, trying to merge it with its adjacent Nodes.
    pub(crate) unsafe fn create_free_node(&mut self, block_ptr: *mut u8, initial_size: usize) {
        let root_ptr = if let Some(ptr) = &self.free_root {
            ptr.load(Ordering::Acquire)
        } else {
            // No root pointer registered yet: no further defragmentation processing can be done.
            // Write the node in place and set it as root.
            let node = Node {
                size: initial_size,
                next_ptr: None,
            };
            ptr::write(block_ptr as *mut Node, node);

            self.free_root = Some(AtomicPtr::new(block_ptr));

            return;
        };

        // Iterate over nodes linked list, searching for correct location to write node (sorted by pointer adress).
        let (previous_ptr, next_ptr) = self.find_insertion_point(block_ptr, root_ptr);

        // Once this place is found, try to merge adjacent blocks.
        let (node, dest_ptr) =
            self.try_merge_nodes(block_ptr, initial_size, previous_ptr, next_ptr);
        ptr::write(dest_ptr as *mut Node, node);

        if previous_ptr.is_none() {
            // Replace root
            self.free_root = Some(AtomicPtr::new(dest_ptr));
        }
    }

    /// Find the new Node location, which is adjacent to one or two Nodes, sorted by memory adress.
    ///
    /// **Returns**:
    /// - Optional previous Node pointer
    /// - Optional next Node pointer
    ///
    /// **Note**: returned pointer options can't be both None.
    pub(crate) unsafe fn find_insertion_point(
        &self,
        block_ptr: *const u8,
        root_ptr: *const u8,
    ) -> (Option<*const u8>, Option<*const u8>) {
        if block_ptr < root_ptr {
            return (None, Some(root_ptr));
        }

        let mut previous_node_ptr = root_ptr;
        let mut previous_node: Node;
        loop {
            previous_node = ptr::read(previous_node_ptr as *const Node);
            previous_node_ptr = match previous_node.next_ptr {
                Some(ptr) if block_ptr < ptr => {
                    // Found the place to store the new node
                    return (Some(previous_node_ptr), Some(ptr));
                }
                Some(ptr) => {
                    // Iterate
                    ptr
                }
                None => {
                    // Reached the end of the list
                    return (Some(previous_node_ptr), None);
                }
            };
        }
    }

    /// Try to merge adjacent nodes into one.
    ///
    /// **Returns**:
    /// - Computed Node structure
    /// - Memory location to write the Node structure to
    pub(crate) unsafe fn try_merge_nodes(
        &self,
        block_ptr: *const u8,
        block_size: usize,
        previous_ptr: Option<*const u8>,
        next_ptr: Option<*const u8>,
    ) -> (Node, *mut u8) {
        let mut node = Node {
            size: block_size,
            next_ptr: None, // Will be set later in this function
        };

        let mut new_ptr = block_ptr;

        if let Some(ptr) = previous_ptr {
            let previous = ptr::read(ptr as *const Node);
            if new_ptr == ptr.add(previous.size) {
                // Merge with previous
                new_ptr = ptr;
                node.size += previous.size;
            }
            node.next_ptr = previous.next_ptr;
        }

        if let Some(ptr) = next_ptr {
            if new_ptr.add(node.size) == ptr {
                let next = ptr::read(ptr as *const Node);
                // Merge with next (don't update node pointer)
                node.size += next.size;
                node.next_ptr = next.next_ptr;
            } else {
                node.next_ptr = next_ptr;
            }
        }

        (node, new_ptr as *mut u8)
    }
}
