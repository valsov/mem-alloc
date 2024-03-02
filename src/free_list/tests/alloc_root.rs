use std::{
    alloc::{GlobalAlloc, Layout, System},
    ptr::{self, null_mut},
    sync::atomic::{AtomicPtr, Ordering},
};

use crate::free_list::{alloc_root::*, node::Node};

#[test]
fn create_free_node_no_root_becomes_root() {
    let mut alloc_data = init_allocator::<128>(vec![
        TestNode {
            size: 32,
            free: false,
        },
        TestNode {
            size: 32,
            free: false,
        },
        TestNode {
            size: 64,
            free: false,
        },
    ]);

    unsafe {
        alloc_data
            .allocator
            .create_free_node(alloc_data.ptr_collection[1] as *mut u8, 32)
    };

    assert_eq!(
        alloc_data.ptr_collection[1],
        alloc_data
            .allocator
            .free_root
            .unwrap()
            .load(Ordering::Acquire)
    );
}

#[test]
fn create_free_node_no_previous_node_becomes_root() {
    let mut alloc_data = init_allocator::<128>(vec![
        TestNode {
            size: 32,
            free: false,
        },
        TestNode {
            size: 32,
            free: false,
        },
        TestNode {
            size: 64,
            free: true, // Current root
        },
    ]);

    unsafe {
        alloc_data
            .allocator
            .create_free_node(alloc_data.ptr_collection[1] as *mut u8, 32)
    };

    assert_eq!(
        alloc_data.ptr_collection[1],
        alloc_data
            .allocator
            .free_root
            .unwrap()
            .load(Ordering::Acquire)
    );
}

#[test]
fn create_free_node_previous_node_exists_doesnt_become_root() {
    let mut alloc_data = init_allocator::<128>(vec![
        TestNode {
            size: 32,
            free: true, // Current root
        },
        TestNode {
            size: 32,
            free: false,
        },
        TestNode {
            size: 64,
            free: false,
        },
    ]);

    unsafe {
        alloc_data
            .allocator
            .create_free_node(alloc_data.ptr_collection[1] as *mut u8, 32)
    };

    assert_eq!(
        alloc_data.ptr_collection[0], // Still old root
        alloc_data
            .allocator
            .free_root
            .unwrap()
            .load(Ordering::Acquire)
    );
}

#[test]
fn find_insertion_point_at_root() {
    let alloc_data = init_allocator::<128>(vec![
        TestNode {
            size: 32,
            free: false,
        },
        TestNode {
            size: 32,
            free: true,
        },
        TestNode {
            size: 64,
            free: true,
        },
    ]);

    // Find insertion point for first node
    let (previous, next) = unsafe {
        alloc_data.allocator.find_insertion_point(
            alloc_data.ptr_collection[0],
            alloc_data.free_root_ptr.unwrap(),
        )
    };

    assert_eq!(None, previous);

    let next_ptr = next.unwrap();
    assert_eq!(alloc_data.ptr_collection[1], next_ptr);
}

#[test]
fn find_insertion_point_between_nodes() {
    let alloc_data = init_allocator::<128>(vec![
        TestNode {
            size: 32,
            free: true,
        },
        TestNode {
            size: 32,
            free: false,
        },
        TestNode {
            size: 64,
            free: true,
        },
    ]);

    // Find insertion point for second node
    let (previous, next) = unsafe {
        alloc_data.allocator.find_insertion_point(
            alloc_data.ptr_collection[1],
            alloc_data.free_root_ptr.unwrap(),
        )
    };

    let previous_ptr = previous.unwrap();
    assert_eq!(alloc_data.ptr_collection[0], previous_ptr);

    let next_ptr = next.unwrap();
    assert_eq!(alloc_data.ptr_collection[2], next_ptr);
}

#[test]
fn find_insertion_point_at_end() {
    let alloc_data = init_allocator::<128>(vec![
        TestNode {
            size: 32,
            free: true,
        },
        TestNode {
            size: 32,
            free: true,
        },
        TestNode {
            size: 64,
            free: false,
        },
    ]);

    // Find insertion point for first node
    let (previous, next) = unsafe {
        alloc_data.allocator.find_insertion_point(
            alloc_data.ptr_collection[2],
            alloc_data.free_root_ptr.unwrap(),
        )
    };

    let previous_ptr = previous.unwrap();
    assert_eq!(alloc_data.ptr_collection[1], previous_ptr);

    assert_eq!(None, next);
}

#[test]
fn try_merge_nodes_can_merge_previous() {
    let alloc_data = init_allocator::<128>(vec![
        TestNode {
            size: 32,
            free: true,
        },
        TestNode {
            size: 32,
            free: false,
        },
        TestNode {
            size: 32,
            free: false,
        },
        TestNode {
            size: 32,
            free: true,
        },
    ]);

    // Try to merge second node
    let (node_result, destination_ptr) = unsafe {
        alloc_data.allocator.try_merge_nodes(
            alloc_data.ptr_collection[1],
            32,
            Some(alloc_data.ptr_collection[0]),
            Some(alloc_data.ptr_collection[3]),
        )
    };

    assert_eq!(32 * 2, node_result.size);
    assert_eq!(alloc_data.ptr_collection[3], node_result.next_ptr.unwrap());
    assert_eq!(alloc_data.ptr_collection[0], destination_ptr);
}

#[test]
fn try_merge_nodes_can_merge_previous_none_next() {
    let alloc_data = init_allocator::<128>(vec![
        TestNode {
            size: 32,
            free: true,
        },
        TestNode {
            size: 32,
            free: false,
        },
        TestNode {
            size: 32,
            free: false,
        },
        TestNode {
            size: 32,
            free: false,
        },
    ]);

    // Try to merge second node
    let (node_result, destination_ptr) = unsafe {
        alloc_data.allocator.try_merge_nodes(
            alloc_data.ptr_collection[1],
            32,
            Some(alloc_data.ptr_collection[0]),
            None,
        )
    };

    assert_eq!(32 * 2, node_result.size);
    assert_eq!(None, node_result.next_ptr);
    assert_eq!(alloc_data.ptr_collection[0], destination_ptr);
}

#[test]
fn try_merge_nodes_can_merge_next() {
    let alloc_data = init_allocator::<128>(vec![
        TestNode {
            size: 32,
            free: true,
        },
        TestNode {
            size: 32,
            free: false,
        },
        TestNode {
            size: 32,
            free: false,
        },
        TestNode {
            size: 32,
            free: true,
        },
    ]);

    // Try to merge third node
    let (node_result, destination_ptr) = unsafe {
        alloc_data.allocator.try_merge_nodes(
            alloc_data.ptr_collection[2],
            32,
            Some(alloc_data.ptr_collection[0]),
            Some(alloc_data.ptr_collection[3]),
        )
    };

    assert_eq!(32 * 2, node_result.size);
    assert_eq!(None, node_result.next_ptr);
    assert_eq!(alloc_data.ptr_collection[2], destination_ptr);
}

#[test]
fn try_merge_nodes_can_merge_next_none_previous() {
    let alloc_data = init_allocator::<128>(vec![
        TestNode {
            size: 32,
            free: false,
        },
        TestNode {
            size: 32,
            free: false,
        },
        TestNode {
            size: 32,
            free: false,
        },
        TestNode {
            size: 32,
            free: true,
        },
    ]);

    // Try to merge third node
    let (node_result, destination_ptr) = unsafe {
        alloc_data.allocator.try_merge_nodes(
            alloc_data.ptr_collection[2],
            32,
            None,
            Some(alloc_data.ptr_collection[3]),
        )
    };

    assert_eq!(32 * 2, node_result.size);
    assert_eq!(None, node_result.next_ptr);
    assert_eq!(alloc_data.ptr_collection[2], destination_ptr);
}

#[test]
fn try_merge_nodes_can_merge_previous_and_next() {
    let alloc_data = init_allocator::<128>(vec![
        TestNode {
            size: 32,
            free: true,
        },
        TestNode {
            size: 32,
            free: false,
        },
        TestNode {
            size: 32,
            free: true,
        },
        TestNode {
            size: 32,
            free: false,
        },
    ]);

    // Try to merge second node
    let (node_result, destination_ptr) = unsafe {
        alloc_data.allocator.try_merge_nodes(
            alloc_data.ptr_collection[1],
            32,
            Some(alloc_data.ptr_collection[0]),
            Some(alloc_data.ptr_collection[2]),
        )
    };

    assert_eq!(32 * 3, node_result.size);
    assert_eq!(None, node_result.next_ptr);
    assert_eq!(alloc_data.ptr_collection[0], destination_ptr);
}

#[test]
fn try_merge_nodes_cannot_merge_any() {
    let alloc_data = init_allocator::<256>(vec![
        TestNode {
            size: 32,
            free: true,
        },
        TestNode {
            size: 32,
            free: false,
        },
        TestNode {
            size: 32,
            free: true,
        },
        TestNode {
            size: 32,
            free: false,
        },
        TestNode {
            size: 32,
            free: true,
        },
    ]);

    // Try to merge third node (middle)
    let (node_result, destination_ptr) = unsafe {
        alloc_data.allocator.try_merge_nodes(
            alloc_data.ptr_collection[2],
            32,
            Some(alloc_data.ptr_collection[0]),
            Some(alloc_data.ptr_collection[4]),
        )
    };

    // No change was made
    assert_eq!(32, node_result.size);
    assert_eq!(alloc_data.ptr_collection[4], node_result.next_ptr.unwrap());
    assert_eq!(alloc_data.ptr_collection[2], destination_ptr);
}

#[test]
fn try_merge_nodes_cannot_merge_any_none_previous_and_next() {
    let alloc_data = init_allocator::<256>(vec![
        TestNode {
            size: 32,
            free: false,
        },
        TestNode {
            size: 32,
            free: true,
        },
        TestNode {
            size: 32,
            free: false,
        },
    ]);

    // Try to merge third node (middle)
    let (node_result, destination_ptr) = unsafe {
        alloc_data
            .allocator
            .try_merge_nodes(alloc_data.ptr_collection[1], 32, None, None)
    };

    // No change was made
    assert_eq!(32, node_result.size);
    assert_eq!(None, node_result.next_ptr);
    assert_eq!(alloc_data.ptr_collection[1], destination_ptr);
}

/// Test utility function to generate an allocator populated with the given nodes
///
/// **Notes**:
/// - Caller must ensure there is enough space to store all nodes
/// - A node should be at least as large as a Node layout size
fn init_allocator<const S: usize>(nodes: Vec<TestNode>) -> AllocatorData {
    // Allocate arena
    let layout = Layout::new::<[u8; S]>();
    let arena_ptr = unsafe { GlobalAlloc::alloc(&System, layout) };
    let mut node_ptr_collection = Vec::new();

    // Build allocation nodes, starting from the end to link nodes without having to look ahead
    let mut free_root = null_mut();
    let mut last_free_ptr = None;
    let mut current_ptr = unsafe { arena_ptr.add(S) };
    let node_layout_size = Layout::new::<Node>().size();
    for node in nodes.iter().rev() {
        if node.size < node_layout_size {
            panic!("node size can't be less than that of a Node layout size")
        }

        // Move cursor to start of block
        current_ptr = unsafe { current_ptr.sub(node.size) };
        node_ptr_collection.push(current_ptr as *const u8);

        if !node.free {
            // Allocated block, nothing to do
            continue;
        }

        // Update the free root as we move to the start of the arena
        free_root = current_ptr;

        // Write node in arena
        let alloc_node = Node {
            size: node.size,
            next_ptr: last_free_ptr,
        };
        unsafe { ptr::write(current_ptr as *mut Node, alloc_node) };

        last_free_ptr = Some(current_ptr);
    }

    node_ptr_collection.reverse(); // Nodes were added in reverse order, reverse back

    let (atomic_root, free_root_ptr) = if free_root.is_null() {
        (None, None)
    } else {
        (
            Some(AtomicPtr::new(free_root)),
            Some(free_root as *const u8),
        )
    };
    AllocatorData {
        allocator: AllocatorRoot {
            free_root: atomic_root,
        },
        ptr_collection: node_ptr_collection,
        free_root_ptr,
    }
}

struct AllocatorData {
    allocator: AllocatorRoot,
    ptr_collection: Vec<*const u8>,
    free_root_ptr: Option<*const u8>,
}

struct TestNode {
    size: usize,
    free: bool,
}
