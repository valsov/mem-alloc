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
static ALLOCATOR: FreeListAllocator = FreeListAllocator::new();

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
            let alloc_padding = compute_padding(align, ptr);
            // [ PADDING | VALUE | PADDING_COUNT (usize) ]
            if alloc_padding + size + USIZE_LAYOUT_SIZE <= self.size {
                Ok(alloc_padding)
            } else {
                // Padding causes the allocation to fail: not enough bytes available
                Err(())
            }
        }
    }
}

// todo: implement iterator
struct AllocatorRoot {
    root_ptr: AtomicPtr<u8>,
    free_root: Option<AtomicPtr<u8>>,
}

// Use Lazy to circumvent const function limitation -> can't call ptr::write inside, this defers it to first usage
struct FreeListAllocator {
    allocator: Lazy<Mutex<AllocatorRoot>>,
}

impl FreeListAllocator {
    const fn new() -> Self {
        FreeListAllocator {
            allocator: Lazy::new(|| {
                let layout = Layout::new::<[u8; ARENA_SIZE]>();
                let arena_ptr = unsafe { GlobalAlloc::alloc(&System, layout) };

                // Write root node at the start of the arena
                let root_node = Node {
                    size: ARENA_SIZE - 64,
                    next_ptr: None,
                };
                // todo: remove these tests
                let test1_next_ptr = unsafe { arena_ptr.add(24) };
                let test_node1 = Node {
                    size: 24,
                    next_ptr: Some(test1_next_ptr),
                };
                let test2_next_ptr = unsafe { arena_ptr.add(64) };
                let test_node2 = Node {
                    size: 40,
                    next_ptr: Some(test2_next_ptr),
                };

                unsafe {
                    ptr::write(arena_ptr as *mut Node, test_node1);
                    ptr::write(test1_next_ptr as *mut Node, test_node2);
                    ptr::write(test2_next_ptr as *mut Node, root_node);
                };

                Mutex::new(AllocatorRoot {
                    root_ptr: AtomicPtr::new(arena_ptr),
                    free_root: Some(AtomicPtr::new(arena_ptr)),
                })
            }),
        }
    }
}

impl AllocatorRoot {
    fn split_alloc(
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

        // [ PADDING | VALUE | PADDING_COUNT (usize) | FREE_NODE ]
        let new_node: Option<Node> =
            if current.size > size + padding + USIZE_LAYOUT_SIZE + NODE_LAYOUT_SIZE {
                // Split the area into allocated and free
                Some(Node {
                    next_ptr: None,
                    size: current.size - padding - size - USIZE_LAYOUT_SIZE,
                })
            } else {
                None
            };

        let alloc_ptr = prev_node.next_ptr.unwrap().cast_mut();

        // Write padding count after value
        unsafe { ptr::write(alloc_ptr.add(size + padding) as *mut usize, padding) }

        // Add free node
        if let Some(mut node) = new_node {
            // Split the area into allocated and free
            node.next_ptr = current.next_ptr;
            let new_free_ptr = unsafe {
                let free_ptr = alloc_ptr.add(size + padding + USIZE_LAYOUT_SIZE);
                ptr::write(free_ptr as *mut Node, node);
                free_ptr as *const u8
            };
            prev_node.next_ptr = Some(new_free_ptr);
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
        let mut node = unsafe { ptr::read(node_ptr.load(Ordering::Acquire) as *const Node) };
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
            node = unsafe { ptr::read(node_ptr as *const Node) };
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
        todo!()
    }
}

fn compute_padding(align: usize, ptr: usize) -> usize {
    (align - (ptr % align)) % align
}
