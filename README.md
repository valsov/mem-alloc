# Rust memory allocators

This project features two memory allocators, which are behind feature flags:
- [Free list allocator](#free-list-allocator): `features = ["free_list"]`
- [Bump allocator](#bump-allocator): `features = ["bump"]`

## Free list allocator

This allocator can be used as the `#[global_allocator]` for any rust program. It is able to allocate any size of data as long as it is able to fit inside the allocated arena.

The free list allocator keeps a free root pointer and stores all available blocks in a sorted (by pointer value) linked list, starting at the root pointer.

### Setup

```rust
use allocator::free_list::FreeListAllocator;

// Where <2048> is the required arena size
#[global_allocator]
static ALLOCATOR: FreeListAllocator<2048> = FreeListAllocator::new();
```

### Allocation
Each time a value needs allocation, it iterates over free nodes until it finds a suitable one (with enough size) and adds allocation metadata at the end of the block. If there is enough space left after the metadata, it writes a new free node there to reference the remaining space.
The allocation space is formatted as one of the following:
- | PAD . ALLOC . ALLOC_METADATA . FILL_PAD |
- | PAD . ALLOC . ALLOC_METADATA . FILL_PAD . FREE_NODE |
#### Blocks
- PAD: padding to respect the value alignment requirements
- ALLOC: space for the required value to be allocated
- ALLOC_METADATA: struct containing references to allocation paddings
	- Added padding count (PAD size), may be 0
	- Additional padding count (FILL_PAD size), may be 0
- FILL_PAD: additional padding after the allocated block to fill size up to a node space (this is mandatory for deallocation process: must have enough space to allocate a free node in place of this)
- FREE_NODE: optional free Node instance if there is enough size to place it

### Deallocation
At deallocation, it iterates over free nodes until it finds the correct place for the new node to be placed, in a sorted manner. It can be the new free node root, placed in between two nodes, or at the end of all nodes. The new node is written to memory and is placed in the linked list.
#### Defragmentation
Free list allocators are subject to fragmentation because each time it deallocates a value, a new free node is created, leading to a lot of nodes being created, becoming smaller and smaller after each allocation.
This problem is solved by sorting the nodes linked list by memory address. This allows to check the previous and next nodes address and size, merging them with the newly created free node if they are adjacent in memory.

## Bump allocator

Simple but fast allocator that pushes values into a memory block. Its downside is not being able to drop individual values.

### Usage

```rust
use allocator::bumper::BumpAllocator;

fn main() {
	// Init
	let bump = BumpAllocator::<2048>::new();
	// Allocate a variable
	let var_a = bump.allocate(123); // &mut i32
	
	// Deallocate all allocated variables
	// Use true as parameter value to write 0 at previously
	// allocated bytes (memory wipe)
	bump.dealloc_all(false);
}
```

### Allocation
Each time a value needs allocation, the allocator writes it at the address of the end pointer. The pointer is then simply incremented to point just after the newly allocated value.

### Deallocation
The only deallocation capability is as a bulk, this deallocates all allocated values in the arena. This is really fast because it only resets the allocation pointer to the start of the arena.
The `dealloc_all` function takes a boolean argument to optionally wipe the previously allocated memory by writing 0 in previous bytes.