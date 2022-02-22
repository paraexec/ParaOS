use crate::serial::{Serial, COM1};
use core::fmt::write;

#[allow(dead_code)]
#[inline]
pub fn panic(info: &core::panic::PanicInfo) -> ! {
    panic_print(info);
    loop {}
}

#[inline]
pub(crate) fn panic_print(info: &core::panic::PanicInfo) {
    let mut serial = Serial(&COM1);
    serial.init();
    write(&mut serial, format_args!("\n{}\n", info)).expect("serial output");
}
