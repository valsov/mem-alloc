use once_cell::sync::Lazy;
use std::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    ptr::{self, null_mut},
    sync::Mutex,
};

const ARENA_SIZE: usize = 128 * 1024;

//#[global_allocator]
//static ALLOCATOR: FreeListAllocator = FreeListAllocator::new();

struct Node {
    size: usize,
    next_ptr: Option<*const u8>,
}

impl Node {
    // Returns allocation padding
    fn matches_requirements(&self, size: usize, align: usize, ptr: usize) -> Result<usize, ()> {
        if size > self.size {
            // Not enough bytes available
            Err(())
        } else {
            let alloc_padding = (align - (ptr % align)) % align;
            if ptr + alloc_padding + size <= self.size {
                Ok(alloc_padding)
            } else {
                // Padding causes the allocation to fail: not enough bytes available
                Err(())
            }
        }
    }
}

struct AllocatorRoot {
    arena: UnsafeCell<[u8; ARENA_SIZE]>,
    free_root: Option<Node>,
}

// Use Lazy to circumvent const function limitation -> can't set &mut variable inside (even if &'static), this defers it to first usage
// Use a Mutex for 2 purposes:
// - allow mutability even in GlobalAlloc functions which take a &Self instead of &mut Self
// - make the allocator thread safe
struct FreeListAllocator {
    allocator: Lazy<Mutex<AllocatorRoot>>,
}

impl FreeListAllocator {
    const fn new() -> Self {
        FreeListAllocator {
            allocator: Lazy::new(|| {
                Mutex::new(AllocatorRoot {
                    arena: UnsafeCell::new([0; ARENA_SIZE]),
                    free_root: Some(Node {
                        size: ARENA_SIZE,
                        next_ptr: None,
                    }),
                })
            }),
        }
    }
}

unsafe impl GlobalAlloc for FreeListAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let allocator = self.allocator.lock().unwrap();
        let mut node = match &allocator.free_root {
            Some(n) => n,
            None => return null_mut(), // No memory available
        };

        let size = layout.size();
        let align = layout.align();

        if let Ok(_padding) = node.matches_requirements(size, align, node as *const Node as usize) {
            todo!("alloc and if padding -> maybe can add free node before -> remove current from free list")
        }

        // Iterate over free nodes until one matches size requirements
        let mut previous_node = node;
        while let Some(node_ptr) = node.next_ptr {
            node = unsafe { &*(node_ptr as *const Node) }; // Get Node at pointer
            if let Ok(_padding) = node.matches_requirements(size, align, node_ptr as usize) {
                todo!("alloc and if padding -> maybe can add free node before -> remove current from free list")
            }

            previous_node = node;
        }

        // Failed to find a suitable space
        null_mut()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        todo!()
    }
}
