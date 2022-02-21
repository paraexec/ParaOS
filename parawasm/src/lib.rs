#![cfg_attr(all(not(test), not(feature = "test")), no_std)]

extern crate alloc;

pub trait Compiler {
    type Error;
    type Module;
    fn compile(&self, module: &[u8]) -> Result<Self::Module, Self::Error>;
}

pub mod x86_64;
