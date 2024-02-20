use std::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    ptr::{self, null_mut},
    sync::atomic::{AtomicUsize, Ordering},
};

/// Issues:
/// - bump() parameter is first allocated to the stack, not optimal
/// - alignment is not taken into account for simplicity

pub struct BumpAllocator<const N: usize> {
    arena: UnsafeCell<[u8; N]>,
    allocated: AtomicUsize,
}

impl<const N: usize> BumpAllocator<N> {
    pub const fn new() -> Self {
        Self {
            arena: UnsafeCell::new([0x0; N]),
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
        let size: usize = layout.size();
        let mut start = 0;

        if self
            .allocated
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |allocated| {
                if size > N - allocated {
                    None
                } else {
                    start = allocated;
                    Some(allocated + size)
                }
            })
            .is_err()
        {
            return null_mut();
        }

        self.arena.get().cast::<u8>().add(start)
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // No per-value deallocation, only full deallocation is available
    }
}
