use once_cell::sync::Lazy;
use std::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    ptr::null_mut,
    sync::Mutex,
};

const ARENA_SIZE: usize = 128 * 1024;

//#[global_allocator]
//static ALLOCATOR: FreeListAllocator = FreeListAllocator::new();

struct AllocatorRoot {
    arena: UnsafeCell<[u8; ARENA_SIZE]>,
    free_root: Option<Node>,
}

struct Node {
    size: usize,
    next_ptr: Option<*mut u8>,
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
        let node = match &allocator.free_root {
            Some(n) => n,
            None => return null_mut(), // No memory available
        };

        let size = layout.size();
        let align = layout.align();

        // Iterate over free nodes until one matches size requirements
        while let Some(node_ptr) = node.next_ptr {}

        todo!()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        todo!()
    }
}

// Returns allocation padding
fn matches_requirements(
    available: usize,
    size: usize,
    align: usize,
    ptr: usize,
) -> Result<usize, ()> {
    if size > available {
        // Not enough bytes available
        Err(())
    } else {
        let alloc_padding = (align - (ptr % align)) % align;
        if ptr + alloc_padding + size <= available {
            Ok(alloc_padding)
        } else {
            // Padding causes the allocation to fail: not enough bytes available
            Err(())
        }
    }
}

/*
if self
    .allocated
    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |allocated| {
        if size > N - allocated {
            // Not enough bytes available
            None
        } else {
            let start_padding = (align - (allocated % align)) % align;
            start = allocated + start_padding;
            Some(start + size)
        }
    })
    .is_err()
{
    return null_mut();
}

// Point to the start of the free bytes
self.arena.lock().unwrap().as_mut_ptr().add(start)
*/
