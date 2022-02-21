use byteorder::{ByteOrder, LittleEndian};
use core::cell::RefCell;
use core::mem::size_of;
use core::ops::{Deref, DerefMut};
use iced_x86::code_asm::{r10, CodeAssembler};
use iced_x86::IcedError;
use parawasm::x86_64::{AssembledModule, FunctionIdentifier};
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::rc::Rc;
use unicorn_engine::unicorn_const::{uc_error, Permission};
use unicorn_engine::RegisterX86::{R10, RSP};
use unicorn_engine::{RegisterX86, Unicorn};

pub use unicorn_engine::RegisterX86::*;

#[derive(Debug)]
pub enum Error {
    EmulationError(uc_error),
    InternalAssemblyError(IcedError),
    FunctionNotFound,
}

impl From<uc_error> for Error {
    fn from(err: uc_error) -> Self {
        Error::EmulationError(err)
    }
}

impl From<IcedError> for Error {
    fn from(err: IcedError) -> Self {
        Error::InternalAssemblyError(err)
    }
}

pub struct Emulator<'a> {
    emulator: Unicorn<'a, ()>,
    module_offset: u64,
    trampoline_len: u64,
    trampoline_offset: u64,
    modules: Vec<Rc<RefCell<Module>>>,
}

impl<'a> Emulator<'a> {
    pub fn new() -> Result<Self, Error> {
        let mut emulator = Unicorn::new(
            unicorn_engine::unicorn_const::Arch::X86,
            unicorn_engine::unicorn_const::Mode::MODE_64,
        )?;

        let initial_offset = 0x0;
        // Map memory
        emulator.mem_map(initial_offset, 128 * 1024 * 1024, Permission::ALL)?;

        // Trampoline
        let mut assembler = CodeAssembler::new(64)?;
        assembler.call(r10)?;
        assembler.nop()?;
        let trampoline = assembler.assemble(0)?;

        emulator.mem_write(initial_offset, &trampoline)?;

        // Set up stack at the top
        emulator.reg_write(RSP as i32, 128 * 1024 * 1024 - 1)?;

        Ok(Self {
            emulator,
            module_offset: initial_offset + trampoline.len() as u64,
            trampoline_len: trampoline.len() as u64,
            trampoline_offset: initial_offset,
            modules: vec![],
        })
    }

    pub fn add_module(&mut self, module: AssembledModule) -> Result<Rc<RefCell<Module>>, Error> {
        self.emulator
            .mem_write(self.module_offset as u64, module.binary())?;
        let module_len = module.binary().len();
        let emu_module = Module {
            offset: self.module_offset,
            module,
            executed_instructions: BTreeMap::new(),
        };
        self.module_offset += module_len as u64;
        let new_module = Rc::new(RefCell::new(emu_module));
        self.modules.push(new_module.clone());
        Ok(new_module)
    }

    fn update_module(&mut self, module: Rc<RefCell<Module>>) -> Result<(), Error> {
        self.emulator
            .mem_write(module.borrow().offset, module.borrow().module.binary())?;
        Ok(())
    }

    pub fn add_memory(&mut self, mem: &[u8]) -> Result<u64, Error> {
        let offset = self.module_offset as u64;
        self.emulator.mem_write(offset, mem)?;
        self.module_offset += mem.len() as u64;
        Ok(offset)
    }

    pub fn call_function<I: FunctionIdentifier>(
        &mut self,
        module: Rc<RefCell<Module>>,
        identifier: I,
    ) -> Result<(), Error> {
        eprintln!("Module assembly:");
        module
            .borrow()
            .dump_asm(self.trampoline_offset + self.trampoline_len);

        let function_offset = module
            .borrow()
            .module
            .function_entry_point(identifier)
            .ok_or(Error::FunctionNotFound)? as u64;
        let module_offset = module.borrow().offset;
        for module in self.modules.clone() {
            self.update_module(module)?;
        }
        let modules = self.modules.clone();
        let hook = self
            .emulator
            .add_code_hook(0, u64::MAX, move |_emu, addr, _| {
                let matching_module = modules.iter().find_map(|module_candidate| {
                    let candidate_begin = module_candidate.borrow().offset;
                    let candidate_end = module_candidate.borrow().offset
                        + (module_candidate.borrow().module.binary().len() as u64);
                    if addr >= candidate_begin && addr <= candidate_end {
                        Some(module_candidate)
                    } else {
                        None
                    }
                });
                if let Some(module) = matching_module {
                    let offset = module.borrow().offset;
                    if let Ok(mut borrowed_module) = module.try_borrow_mut() {
                        match borrowed_module
                            .executed_instructions
                            .entry((addr - offset) as usize)
                        {
                            Entry::Vacant(ve) => {
                                ve.insert(1);
                            }
                            Entry::Occupied(mut oe) => *oe.get_mut() += 1,
                        }
                    }
                }
            })?;

        self.emulator
            .reg_write(R10 as i32, module_offset + function_offset)?;

        self.emulator.emu_start(
            self.trampoline_offset,
            self.trampoline_offset + self.trampoline_len,
            0,
            0,
        )?;
        self.emulator.remove_hook(hook)?;
        Ok(())
    }

    pub fn pop(&mut self) -> Result<u64, Error> {
        let mut stack = self.emulator.reg_read(RSP as i32)?;
        let mut buf = [0; size_of::<u64>() as usize];
        self.emulator.mem_read(stack, &mut buf)?;
        stack += size_of::<u64>() as u64;
        self.emulator.reg_write(RSP as i32, stack)?;
        Ok(LittleEndian::read_u64(&mut buf))
    }

    pub fn push(&mut self, value: u64) -> Result<(), Error> {
        let mut stack = self.emulator.reg_read(RSP as i32)?;
        stack -= size_of::<u64>() as u64;
        self.emulator.reg_write(RSP as i32, stack)?;
        let mut buf = [0; size_of::<u64>() as usize];
        LittleEndian::write_u64(&mut buf, value);
        self.emulator.mem_write(stack, &buf)?;
        Ok(())
    }

    pub fn read_register(&self, register: RegisterX86) -> Result<u64, Error> {
        Ok(self.emulator.reg_read(register as i32)?)
    }

    pub fn write_register(&mut self, register: RegisterX86, value: u64) -> Result<(), Error> {
        Ok(self.emulator.reg_write(register as i32, value)?)
    }
}

pub struct Module {
    offset: u64,
    module: AssembledModule,
    executed_instructions: BTreeMap<usize, usize>,
}

impl Deref for Module {
    type Target = AssembledModule;

    fn deref(&self) -> &Self::Target {
        &self.module
    }
}

impl DerefMut for Module {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.module
    }
}

impl Module {
    pub fn instruction_execution_count(&self, offset: usize) -> usize {
        self.executed_instructions
            .get(&offset)
            .map(|v| *v)
            .unwrap_or(0)
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }
}
