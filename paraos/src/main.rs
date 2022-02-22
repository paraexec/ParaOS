#![no_std]
#![no_main]
#![feature(core_panic)]
#![feature(default_alloc_error_handler)]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test::test_runner)]
#![cfg_attr(test, reexport_test_harness_main = "test_main")]

mod bootboot;
mod kernel;
mod panic;
mod platform;
mod serial;
#[cfg(test)]
mod test;

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    panic::panic(info);
}

extern "C" {
    #[allow(dead_code)]
    static BOOTBOOT: bootboot::Bootboot;
}

use buddy_system_allocator::LockedHeap;

#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap<32> = LockedHeap::<32>::empty();

#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn _start() -> ! {
    static START: kernel::StartBarrier = kernel::StartBarrier::new();
    kernel::Kernel::new(unsafe { BOOTBOOT.bspid as u32 }, &START).run(|| unsafe {
        for entry in &BOOTBOOT.memory_mappings()[1..] {
            if entry.is_free() && entry.addr() != 0x0 {
                HEAP_ALLOCATOR
                    .lock()
                    .add_to_heap(entry.addr(), entry.addr() + entry.size());
            }
        }
        let mut serial = serial::Serial::new(&serial::COM1);
        serial.init();
        core::fmt::write(
            &mut serial,
            format_args!(
                "Free memory: {}MB\n",
                HEAP_ALLOCATOR.lock().stats_total_bytes() / (1024 * 1024)
            ),
        )
        .expect("serial output");
    });
    loop {}
}
