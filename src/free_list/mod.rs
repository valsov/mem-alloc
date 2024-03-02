use self::{
    alloc_root::AllocatorRoot,
    node::{AllocationMetadata, ALLOCATION_METADATA_LAYOUT_SIZE},
};
use node::Node;
use once_cell::sync::Lazy;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    ptr::{self, null_mut},
    sync::{
        atomic::{AtomicPtr, Ordering},
        Mutex,
    },
};

mod alloc_root;
mod node;
#[cfg(test)]
mod tests;

/// Free list allocator. It handles auto defragmentation on deallocation.
/// The pool size is set using a generic type argument (see usage example).
///
/// ## Usage
/// ```
/// #[global_allocator]
/// static ALLOCATOR: FreeListAllocator<1024> = FreeListAllocator::new();
/// ```
///
/// ## Note
/// Lazy is used to circumvent const function limitation, it allows a call to `ptr::write`.
/// This defers the initialization to first allocation call.
pub struct FreeListAllocator<const S: usize> {
    allocator: Lazy<Mutex<AllocatorRoot>>,
}

impl<const S: usize> FreeListAllocator<S> {
    pub const fn new() -> Self {
        FreeListAllocator {
            allocator: Lazy::new(|| {
                let layout = Layout::new::<[u8; S]>();
                let arena_ptr = unsafe { GlobalAlloc::alloc(&System, layout) };

                // Write root node at the start of the arena
                let root_node = Node {
                    size: S,
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

unsafe impl<const S: usize> GlobalAlloc for FreeListAllocator<S> {
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
        if let Ok(alloc_specs) =
            node.try_get_alloc_specs(size, align, node_ptr.load(Ordering::Acquire))
        {
            return allocator.split_alloc(None, node, alloc_specs);
        }

        // Iterate over free nodes until one matches size requirements
        let mut previous_node = node;
        while let Some(node_ptr) = previous_node.next_ptr {
            node = ptr::read(node_ptr as *const Node);
            if let Ok(alloc_specs) = node.try_get_alloc_specs(size, align, node_ptr) {
                // Allocate in place of the current free node
                return allocator.split_alloc(Some(previous_node), node, alloc_specs);
            }

            previous_node = node;
        }

        // Failed to find a suitable space
        null_mut()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut allocator = self.allocator.lock().unwrap();

        // Get allocation metadata
        let metadata = {
            let metadata_ptr = ptr.add(layout.size());
            ptr::read(metadata_ptr as *mut AllocationMetadata)
        };
        // Get start of block
        let block_ptr = ptr.sub(metadata.align_padding);

        allocator.create_free_node(
            block_ptr,
            metadata.align_padding
                + layout.size()
                + ALLOCATION_METADATA_LAYOUT_SIZE
                + metadata.fill_padding,
        );
    }
}
