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
        if wipe_memory {
            // Write 0 in all allocated array space
            let ptr = self.arena_ptr.load(Ordering::Acquire);
            let len_bytes = self.allocated.load(Ordering::SeqCst) * size_of::<u8>();
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
