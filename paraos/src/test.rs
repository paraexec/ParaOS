use crate::platform::pick_leader_core;
use crate::serial::{Serial, COM1};
use core::fmt::{write, Write};

pub(crate) fn test_runner(tests: &[&dyn Fn()]) {
    let leader = pick_leader_core();
    let mut port = Serial(&COM1);
    port.init();
    if leader {
        write(&mut port, format_args!("Running {} tests\n", tests.len())).expect("serial output");
    }
    for test in tests {
        test();
        if leader {
            port.write_char('.').expect("serial output");
        }
    }
    if leader {
        port.write_char('\n').expect("serial output");
    }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    crate::test_main();

    let leader = pick_leader_core();
    if leader {
        unsafe {
            // qemu exit (isa-debug-exit)
            x86::io::outb(0x501, 0);
        }
        unreachable!()
    } else {
        loop {}
    }
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    crate::panic::panic_print(info);
    unsafe {
        x86::io::outb(0x501, 1);
    }
    loop {}
}
