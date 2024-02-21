use std::{
    alloc::{GlobalAlloc, Layout},
    ptr::{self, null_mut},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};

/// Issue(s):
/// - bump() value parameter is first allocated to the stack, not optimal

pub struct BumpAllocator<const N: usize> {
    arena: Mutex<Box<[u8; N]>>,
    allocated: AtomicUsize,
}

impl<const N: usize> BumpAllocator<N> {
    pub fn new() -> Self {
        Self {
            arena: Mutex::new(Box::new([0x0; N])),
            allocated: AtomicUsize::new(0),
        }
    }

    pub fn bump<'a, T>(&self, value: T) -> &'a mut T {
        let layout = Layout::new::<T>();
        let ptr = unsafe { self.alloc(layout) };
        if ptr.is_null() {
            panic!("Custom allocation failed");
        }
        unsafe {
            ptr::write(ptr as *mut T, value);
            (ptr as *mut T).as_mut().unwrap() // Return value at new address
        }
    }

    pub fn dealloc_all(&mut self) {
        self.allocated.store(0, Ordering::SeqCst);
    }
}

unsafe impl<const N: usize> GlobalAlloc for BumpAllocator<N> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();
        let mut start = 0;

        if self
            .allocated
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |allocated| {
                if size > N - allocated {
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

        self.arena.lock().unwrap().as_mut_ptr().add(start)
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // No per-value deallocation, only full deallocation is available
    }
}
