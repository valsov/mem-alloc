use once_cell::sync::Lazy;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    ptr::{self, null_mut},
    sync::{
        atomic::{AtomicPtr, Ordering},
        Mutex,
    },
};

const ARENA_SIZE: usize = 1024;
const NODE_LAYOUT_SIZE: usize = Layout::new::<Node>().size();
const USIZE_LAYOUT_SIZE: usize = Layout::new::<usize>().size();

#[global_allocator]
pub static ALLOCATOR: FreeListAllocator = FreeListAllocator::new();

struct Node {
    next_ptr: Option<*const u8>,
    size: usize,
}

impl Node {
    /// Check if the given parameters are suitable for an allocation in terms of available space.
    ///
    /// **Returns**:
    /// - allocation padding (to add before value)
    fn matches_requirements(&self, size: usize, align: usize, ptr: usize) -> Result<usize, ()> {
        if size > self.size {
            // Not enough bytes available
            Err(())
        } else {
            let alloc_padding = (align - (ptr % align)) % align;

            if alloc_padding + size + USIZE_LAYOUT_SIZE + USIZE_LAYOUT_SIZE <= self.size {
                // Valid if padding + size + alloc metadata can fit inside
                Ok(alloc_padding)
            } else {
                // Padding causes the allocation to fail: not enough bytes available
                Err(())
            }
        }
    }
}

struct AllocatorRoot {
    free_root: Option<AtomicPtr<u8>>,
}

// Use Lazy to circumvent const function limitation -> can't call ptr::write inside, this defers it to first usage
pub struct FreeListAllocator {
    allocator: Lazy<Mutex<AllocatorRoot>>,
}

impl FreeListAllocator {
    pub const fn new() -> Self {
        FreeListAllocator {
            allocator: Lazy::new(|| {
                let layout = Layout::new::<[u8; ARENA_SIZE]>();
                let arena_ptr = unsafe { GlobalAlloc::alloc(&System, layout) };

                // Write root node at the start of the arena
                let root_node = Node {
                    size: ARENA_SIZE,
                    next_ptr: None,
                };

                unsafe {
                    ptr::write(arena_ptr as *mut Node, root_node);
                };

                Mutex::new(AllocatorRoot {
                    free_root: Some(AtomicPtr::new(arena_ptr)),
                })
            }),
        }
    }
}

impl AllocatorRoot {
    /// Allocate memory for the given size and alignment parameters, in place of an existing free Node.
    /// If there is enough space left, add a new free Node with the remaining size.
    ///
    /// **Allocation possibilities**:
    /// - | PAD . ALLOC . PAD_COUNT . FILL_PAD_COUNT . FILL_PAD |
    /// - | PAD . ALLOC . PAD_COUNT . FILL_PAD_COUNT . FILL_PAD . FREE_NODE |
    ///
    /// **Blocks**:
    /// - PAD: padding to respect the value alignment requirements
    /// - ALLOC: space for the required value to be allocated
    /// - PAD_COUNT: added padding count (usize), may be 0
    /// - FILL_PAD_COUNT: additional padding count (usize), may be 0
    /// - FILL_PAD: additional padding after the allocated block to fill size up to a Node space
    /// (this is mandatory for deallocation process: must have enough space to allocate a free Block in place of this)
    /// - FREE_NODE: optional free Node instance if there is enough size to place it
    unsafe fn split_alloc(
        &mut self,
        previous: Option<Node>,
        current: Node,
        size: usize,
        padding: usize,
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

        // Compute sizes
        let alloc_size = padding + size + USIZE_LAYOUT_SIZE + USIZE_LAYOUT_SIZE;
        let fill_padding: usize; // Additional space needed to at least store a Node at deallocation
        let new_node: Option<Node> = if current.size > alloc_size + NODE_LAYOUT_SIZE {
            // Split the area into allocated and free
            if NODE_LAYOUT_SIZE <= alloc_size {
                fill_padding = 0;
            } else {
                fill_padding = NODE_LAYOUT_SIZE - alloc_size;
            }
            Some(Node {
                next_ptr: None, // Will be set later in the function
                size: current.size - alloc_size - fill_padding,
            })
        } else {
            if current.size <= alloc_size {
                fill_padding = 0;
            } else {
                fill_padding = current.size - alloc_size;
            }
            None
        };

        // calculate allocation ptr (current block start + padding)
        let alloc_ptr = prev_node.next_ptr.unwrap().cast_mut().add(padding);

        // Write padding count and fill padding count after value
        let mut ptr_cursor = alloc_ptr.add(size);
        ptr::write(ptr_cursor as *mut usize, padding);

        ptr_cursor = ptr_cursor.add(USIZE_LAYOUT_SIZE);
        ptr::write(ptr_cursor as *mut usize, fill_padding);

        // Add free node
        if let Some(mut node) = new_node {
            // Split the area into allocated and free
            ptr_cursor = ptr_cursor.add(USIZE_LAYOUT_SIZE + fill_padding);
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
    unsafe fn create_node(&mut self, block_ptr: *mut u8, initial_size: usize) {
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
    unsafe fn find_insertion_point(
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
    unsafe fn try_merge_nodes(
        &mut self,
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

unsafe impl GlobalAlloc for FreeListAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut allocator = self.allocator.lock().unwrap();
        let node_ptr = match &allocator.free_root {
            Some(n) => n,
            None => return null_mut(), // No memory available
        };

        let size = layout.size();
        let align = layout.align();

        // Initial node
        let mut node = ptr::read(node_ptr.load(Ordering::Acquire) as *const Node);
        if let Ok(padding) = node.matches_requirements(
            size,
            align,
            node_ptr.load(Ordering::Acquire) as *const Node as usize,
        ) {
            return allocator.split_alloc(None, node, size, padding);
        }

        // Iterate over free nodes until one matches size requirements
        let mut previous_node = node;
        while let Some(node_ptr) = previous_node.next_ptr {
            node = ptr::read(node_ptr as *const Node);
            if let Ok(padding) = node.matches_requirements(size, align, *node_ptr as usize) {
                // Allocate in place of the current free node
                return allocator.split_alloc(Some(previous_node), node, size, padding);
            }

            previous_node = node;
        }

        // Failed to find a suitable space
        null_mut()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut allocator = self.allocator.lock().unwrap();

        // Get start of block
        let padding = {
            let padding_ptr = ptr.add(layout.size());
            ptr::read(padding_ptr as *mut usize)
        };
        let block_ptr = ptr.sub(padding);

        // Get fill padding
        let fill_padding = {
            let fill_padding_ptr = ptr.add(layout.size() + USIZE_LAYOUT_SIZE);
            ptr::read(fill_padding_ptr as *mut usize)
        };

        allocator.create_node(
            block_ptr,
            padding + layout.size() + USIZE_LAYOUT_SIZE + USIZE_LAYOUT_SIZE + fill_padding,
        );
    }
}
