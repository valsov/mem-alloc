use std::{
    alloc::{GlobalAlloc, Layout, System},
    mem::size_of,
    ptr::{self, null_mut},
    sync::atomic::{AtomicPtr, AtomicUsize, Ordering},
};

/// Heap allocator that simply places values after each other and isn't capable of single element deallocation.
///
/// This allocator is really fast and is able to deallocate all elements contained in it even faster.
/// It supports memory wiping, writing 0 in each previously allocated byte.
pub struct BumpAllocator<const N: usize> {
    arena_ptr: AtomicPtr<u8>,
    allocated: AtomicUsize,
}

impl<const N: usize> BumpAllocator<N> {
    /// Create a new instance of bump allocator, initialize the heap memory region for future allocations.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let layout = Layout::new::<[u8; N]>();
        let arena_ptr = unsafe { GlobalAlloc::alloc(&System, layout) };
        Self {
            arena_ptr: AtomicPtr::new(arena_ptr),
            allocated: AtomicUsize::new(0),
        }
    }

    /// Allocate the given value to the heap using bump allocation.
    ///
    /// Known issue: value parameter is first allocated to the stack, which is not optimal.
    pub fn allocate<'a, T>(&self, value: T) -> &'a mut T {
        let layout = Layout::new::<T>();
        let ptr = unsafe { self.alloc(layout) };
        if ptr.is_null() {
            panic!("bump allocation failed");
        }

        unsafe {
            ptr::write(ptr as *mut T, value);
            (ptr as *mut T).as_mut().unwrap() // Return value at new address
        }
    }

    /// Reset the bump allocator, freeing all its space.
    /// This is really fast because it just implies setting the allocation cursor to 0.
    ///
    /// * `wipe_memory`: Set to true to write 0 bytes where memory was allocated, false to leave the memory intact.
    pub fn dealloc_all(&self, wipe_memory: bool) {
        let size = self.allocated.load(Ordering::Acquire);
        if size == 0 {
            // Nothing is currently allocated, can fast return
            return;
        }

        if wipe_memory {
            // Write 0 in all allocated array space
            let ptr = self.arena_ptr.load(Ordering::Acquire);
            let len_bytes = size * size_of::<u8>();
            unsafe {
                ptr::write_bytes(ptr, 0, len_bytes);
            }
        }

        // Reset cursor
        self.allocated.store(0, Ordering::SeqCst);
    }
}

unsafe impl<const N: usize> GlobalAlloc for BumpAllocator<N> {
    /// Allocate memory for a layout.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();
        let mut alloc_offset = 0;

        // Try to update allocated cursor
        if self
            .allocated
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |allocated| {
                if size > N - allocated {
                    // Not enough bytes available
                    None
                } else {
                    let alloc_padding = (align - (allocated % align)) % align;
                    alloc_offset = allocated + alloc_padding;

                    let alloc_end = alloc_offset + size;
                    if alloc_end <= N {
                        Some(alloc_end)
                    } else {
                        // Padding causes the allocation to fail: not enough bytes available
                        None
                    }
                }
            })
            .is_err()
        {
            return null_mut();
        }

        // Point to the start of the free bytes
        self.arena_ptr.load(Ordering::Acquire).add(alloc_offset)
    }

    /// Deallocation of a single element.
    ///
    /// No per-value deallocation, only full deallocation is available.
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[cfg(test)]
mod test {
    use crate::bumper::*;

    #[test]
    #[should_panic]
    fn allocate_not_enough_space_panic() {
        let bumper = BumpAllocator::<2>::new();
        bumper.allocate(123); // i32 has layout size of 4 bytes, which is more than the available space (2 bytes)
    }

    #[test]
    fn allocate_enough_space() {
        let bumper = BumpAllocator::<4>::new();
        let i32_var = bumper.allocate(123);

        assert_eq!(*i32_var, 123);
        let allocated = bumper.allocated.load(Ordering::Acquire);
        assert_eq!(Layout::new::<i32>().size(), allocated)
    }

    #[test]
    fn dealloc_all_empty_no_panic() {
        let bumper = BumpAllocator::<8>::new();
        bumper.dealloc_all(false);
    }

    #[test]
    fn dealloc_all_wipe_memory_empty_no_panic() {
        let bumper = BumpAllocator::<8>::new();
        bumper.dealloc_all(true);
    }

    #[test]
    fn dealloc_all() {
        let bumper = BumpAllocator::<8>::new();
        bumper.allocate(123);

        bumper.dealloc_all(false);
        let allocated = bumper.allocated.load(Ordering::Acquire);
        assert_eq!(0, allocated); // Reset

        // Bytes should still be set to the correct value, no wipe
        let start_ptr = bumper.arena_ptr.load(Ordering::Acquire);
        let stored_i32 = unsafe { ptr::read(start_ptr as *const i32) };
        assert_eq!(123, stored_i32);
    }

    #[test]
    fn dealloc_all_wipe_memory() {
        let bumper = BumpAllocator::<8>::new();
        bumper.allocate(123);

        bumper.dealloc_all(true);
        let allocated = bumper.allocated.load(Ordering::Acquire);
        assert_eq!(0, allocated); // Reset

        // Bytes should be set to 0
        let start_ptr = bumper.arena_ptr.load(Ordering::Acquire);
        let stored_i32 = unsafe { ptr::read(start_ptr as *const i32) };
        assert_eq!(0, stored_i32);
    }
}
