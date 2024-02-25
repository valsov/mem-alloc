use crate::free_list::node::{Node, ALLOCATION_METADATA_LAYOUT_SIZE, NODE_LAYOUT_SIZE};

#[test]
fn try_get_alloc_specs_not_enough_size() {
    let node = Node {
        size: 16,
        next_ptr: None,
    };

    let result = node.try_get_alloc_specs(64, 1, std::ptr::null::<u8>());
    assert!(result.is_err())
}

#[test]
fn try_get_alloc_specs_not_enough_with_padding() {
    let node = Node {
        size: 32,
        next_ptr: None,
    };

    let result = node.try_get_alloc_specs(16, 32, 0x5 as *const u8);
    assert!(result.is_err())
}

#[test]
fn try_get_alloc_specs_not_enough_for_future_node() {
    let node = Node {
        size: 23, // Node layout is 24
        next_ptr: None,
    };

    let result = node.try_get_alloc_specs(4, 1, std::ptr::null::<u8>());
    assert!(result.is_err())
}

#[test]
fn try_get_alloc_specs_can_add_node() {
    let node = Node {
        size: 64,
        next_ptr: None,
    };

    let size = 4;
    let result = node.try_get_alloc_specs(size, 1, std::ptr::null::<u8>());
    assert!(result.is_ok());
    let specs = result.unwrap();
    assert_eq!(0, specs.padding);
    assert_eq!(size, specs.size);
    assert_eq!(
        NODE_LAYOUT_SIZE - size - ALLOCATION_METADATA_LAYOUT_SIZE,
        specs.fill_padding
    );
    assert_eq!(
        node.size
            - specs.padding
            - specs.size
            - ALLOCATION_METADATA_LAYOUT_SIZE
            - specs.fill_padding,
        specs.remaining_size
    );
}

#[test]
fn try_get_alloc_specs_cannot_add_node() {
    let node = Node {
        size: 64,
        next_ptr: None,
    };

    let size = 32;
    let result = node.try_get_alloc_specs(size, 1, std::ptr::null::<u8>());
    assert!(result.is_ok());
    let specs = result.unwrap();
    assert_eq!(0, specs.padding);
    assert_eq!(size, specs.size);
    assert_eq!(
        node.size - specs.padding - size - ALLOCATION_METADATA_LAYOUT_SIZE,
        specs.fill_padding
    );
    assert_eq!(0, specs.remaining_size);
}
