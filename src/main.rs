#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(rust_os::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use rust_os::println;
use rust_os::task::{executor::Executor, keyboard, Task};
use bootloader::{entry_point, BootInfo};
use x86_64::instructions::port::{Port, PortGeneric, ReadWriteAccess};
use core::panic::PanicInfo;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    use rust_os::allocator;
    use rust_os::memory::{self, BootInfoFrameAllocator};
    use x86_64::VirtAddr;

    println!("Hello World{}", "!");
    rust_os::init();

    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_map) };

    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");

    #[cfg(test)]
    test_main();

    let mut status = Port::new(0x1F7);
    for _ in 0..14 {
        unsafe { status.read(); }
    }
    println!("Old status: {}", unsafe { status.read() });
    let mut dselect = Port::new(0x1F6);
    unsafe { dselect.write(0xA0_u8); }
    println!("Selected drive");
    for i in 0..4 {
        let mut p = Port::new(0x1F2 + i);
        unsafe { p.write(0x0_u8); }
    }
    println!("Set ports low");
    for _ in 0..14 {
        unsafe { let s = status.read(); }
    }
    let mut s: u8 = unsafe { status.read() };
    if s == 0 { println!("No drive"); }
    else {
        println!("Drive found");
        println!("{}", s);
        let mut mid: PortGeneric<u8, ReadWriteAccess> = Port::new(0x1F4);
        let mut hi: PortGeneric<u8, ReadWriteAccess> = Port::new(0x1F5);
        unsafe {
            println!("LBAmid: {}, LBAhi: {}", mid.read(), hi.read());
        }

        while (s & 0x80) > 0 { s = unsafe { status.read() }; }
        while (s & 0x8) == 0 { s = unsafe { status.read() }; }
        let mut datap = Port::new(0x1F0);
        let mut data = [0_u16; 256];
        for i in 0..256 { 
            println!("Reading data, {i}/256 done");
            data[i] = unsafe { datap.read() }; 
        }
        println!("{:?}", data);
    }

    let mut executor = Executor::new();
    executor.spawn(Task::new(keyboard::print_keypresses()));
    executor.run();
}

/// This function is called on panic.
#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    rust_os::hlt_loop();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    rust_os::test_panic_handler(info)
}

async fn async_number() -> u32 {
    42
}

async fn example_task() {
    let number = async_number().await;
    println!("async number: {}", number);
}

#[test_case]
fn trivial_assertion() {
    assert_eq!(1, 1);
}
