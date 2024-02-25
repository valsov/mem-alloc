use std::alloc::Layout;

pub const NODE_LAYOUT_SIZE: usize = Layout::new::<Node>().size();
pub const USIZE_LAYOUT_SIZE: usize = Layout::new::<usize>().size();

pub struct Node {
    pub next_ptr: Option<*const u8>,
    pub size: usize,
}

impl Node {
    /// Check if the given parameters are suitable for an allocation in terms of available space.
    /// If the allocation is possible, retrieve allocation specs.
    pub fn try_get_alloc_specs(
        &self,
        size: usize,
        align: usize,
        ptr: usize,
    ) -> Result<AllocationSpecs, ()> {
        if size > self.size {
            // Fast out: not enough bytes available
            return Err(());
        }

        let alloc_padding = (align - (ptr % align)) % align;
        let alloc_size = alloc_padding + size + USIZE_LAYOUT_SIZE + USIZE_LAYOUT_SIZE;

        // Valid if padding + size + alloc metadata can fit inside
        // It also needs to be able to fit a Node once it's deallocated
        if self.size > alloc_size + NODE_LAYOUT_SIZE {
            // Can add a Node after allocation
            let fill_padding = if NODE_LAYOUT_SIZE <= alloc_size {
                // Handle usize overflow
                0
            } else {
                NODE_LAYOUT_SIZE - alloc_size
            };
            Ok(AllocationSpecs {
                padding: alloc_padding,
                size,
                fill_padding,
                remaining_size: self.size - alloc_size - fill_padding,
            })
        } else if alloc_size <= self.size && self.size >= NODE_LAYOUT_SIZE {
            Ok(AllocationSpecs {
                padding: alloc_padding,
                size,
                fill_padding: self.size - alloc_size,
                remaining_size: 0,
            })
        } else {
            // Padding and metadata causes the allocation to fail: not enough bytes available
            Err(())
        }
    }
}

pub struct AllocationSpecs {
    /// Allocation padding (to add before value)
    pub padding: usize,
    /// Size of the value to allocate
    pub size: usize,
    /// Fill padding (to add after metadata)
    pub fill_padding: usize,
    /// Remaining size if it can at least contain a Node
    pub remaining_size: usize,
}
