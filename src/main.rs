use free_list::FreeListAllocator;

mod bumper;
mod free_list;

#[global_allocator]
pub static ALLOCATOR: FreeListAllocator<1024> = FreeListAllocator::new();

fn main() {
    let test1 = Box::new(1115);
    let test2 = Box::new(false);
    let test3 = Box::new(2.3);

    drop(test3);
    drop(test1);
    drop(test2);
}
