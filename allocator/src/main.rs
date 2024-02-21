use bumper::BumpAllocator;

mod bumper;
mod linked_list;

fn main() {
    let mut bump_allocator = BumpAllocator::<1024>::new();
    let a = bump_allocator.allocate(5);
    let test1 = bump_allocator.allocate(Test {
        test_a: 123,
        test_b: true,
    });
    let test2 = bump_allocator.allocate(Test {
        test_a: 456,
        test_b: false,
    });
    println!("{:?}", test1);
    println!("{:?}", test2);
    bump_allocator.dealloc_all(true);

    let test3 = bump_allocator.allocate(Test {
        test_a: 111111111111,
        test_b: true,
    });

    println!("{:?}", test1);
    println!("{:?}", test2);
}

#[derive(Debug)]
struct Test {
    test_a: usize,
    test_b: bool,
}
