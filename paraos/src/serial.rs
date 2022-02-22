use core::fmt::Write;
use spin::{Mutex, MutexGuard};
use x86::io::{inb, outb};

pub struct Port {
    port: u16,
    mutex: Mutex<()>,
}

impl Port {
    pub(crate) const fn new(port: u16) -> Self {
        Self {
            port,
            mutex: Mutex::new(()),
        }
    }

    fn lock(&self) -> MutexGuard<()> {
        self.mutex.lock()
    }
}

pub struct Serial(pub &'static Port);

pub static COM1: Port = Port::new(0x3f8u16);

impl Serial {
    pub fn new(port: &'static Port) -> Self {
        Self(port)
    }

    fn is_transmit_empty(&self) -> bool {
        return unsafe { inb(self.0.port + 5) } & 0x20 != 0;
    }

    pub fn init(&self) {
        self.0.lock();
        unsafe {
            outb(self.0.port + 1, 0x00); // Disable all interrupts
            outb(self.0.port + 3, 0x80); // Enable DLAB (set baud rate divisor)
            outb(self.0.port + 0, 0x03); // Set divisor to 3 (lo byte) 38400 baud
            outb(self.0.port + 1, 0x00); //                  (hi byte)
            outb(self.0.port + 3, 0x03); // 8 bits, no parity, one stop bit
            outb(self.0.port + 2, 0xC7); // Enable FIFO, clear them, with 14-byte threshold
            outb(self.0.port + 4, 0x0B); // IRQs enabled, RTS/DSR set
            outb(self.0.port + 4, 0x0F);
        }
    }
}

impl Write for Serial {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.0.lock();
        for c in s.chars() {
            unsafe {
                outb(self.0.port + 0, c as u8);
            }
        }
        Ok(())
    }

    fn write_char(&mut self, c: char) -> core::fmt::Result {
        self.0.lock();
        while !self.is_transmit_empty() {}
        unsafe {
            outb(self.0.port + 0, c as u8);
        }
        Ok(())
    }
}
