use crate::Compiler;
use alloc::borrow::ToOwned;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use byteorder::{ByteOrder, LittleEndian};
use core::mem::size_of;
use core::ops::{Deref, DerefMut};
use iced_x86::code_asm::{
    dword_ptr, ptr, qword_ptr, r11, r8, r9, rax, rbp, rcx, rdi, rdx, rsi, rsp, AsmRegister64,
    CodeAssembler,
};
use iced_x86::{BlockEncoderOptions, IcedError};
use wasmparser_nostd::*;

mod instructions;
mod optimizer;

trait EncodingSize {
    fn encoding_size(&self) -> u32;
}

impl EncodingSize for Type {
    fn encoding_size(&self) -> u32 {
        match self {
            Type::I32 => 4,
            Type::I64 => 8,
            Type::F32 => 4,
            Type::F64 => 8,
            Type::V128 => 16,
            Type::FuncRef => todo!(),
            Type::ExternRef => todo!(),
            Type::ExnRef => todo!(),
            Type::Func => todo!(),
            Type::EmptyBlockType => todo!(),
        }
    }
}

#[derive(Debug)]
pub enum Error {
    WasmReaderError(BinaryReaderError),
    AssemblerError(IcedError),
}

impl From<BinaryReaderError> for Error {
    fn from(e: BinaryReaderError) -> Self {
        Self::WasmReaderError(e)
    }
}

impl From<IcedError> for Error {
    fn from(e: IcedError) -> Self {
        Self::AssemblerError(e)
    }
}

pub struct X86_64Compiler;

impl core::default::Default for X86_64Compiler {
    fn default() -> Self {
        X86_64Compiler
    }
}

pub struct Module {
    functions: BTreeMap<u32, usize>,
    function_bodies: BTreeMap<u32, usize>,
    function_stack_heights: BTreeMap<u32, u32>,
    exports: BTreeMap<String, u32>,
    imports: BTreeMap<u32, (String, Option<String>, usize)>,
    memories: Vec<MemoryType>,
}

pub struct FunctionIndex(u32);

pub trait FunctionIdentifier {
    fn find_function(&self, module: &Module) -> Option<u32>;
}

impl FunctionIdentifier for u32 {
    fn find_function(&self, module: &Module) -> Option<u32> {
        module.function_bodies.get(self).map(|_| *self)
    }
}

impl FunctionIdentifier for &str {
    fn find_function(&self, module: &Module) -> Option<u32> {
        module
            .exports
            .get(self as &str)
            .and_then(|index| (*index).find_function(module))
    }
}

impl Module {
    fn new() -> Self {
        Self {
            functions: BTreeMap::new(),
            function_bodies: BTreeMap::new(),
            function_stack_heights: BTreeMap::new(),
            exports: BTreeMap::new(),
            imports: BTreeMap::new(),
            memories: Vec::new(),
        }
    }

    fn assembled(self, assembled: Vec<u8>) -> AssembledModule {
        AssembledModule {
            module: self,
            assembled,
        }
    }

    pub fn function_entry_point<I: FunctionIdentifier>(&self, identifier: I) -> Option<usize> {
        identifier
            .find_function(self)
            .and_then(|idx| self.function_bodies.get(&idx).cloned())
    }

    pub fn function_stack_height<I: FunctionIdentifier>(&self, identifier: I) -> Option<u32> {
        identifier
            .find_function(self)
            .and_then(|idx| self.function_stack_heights.get(&idx).cloned())
    }

    pub fn memory_types(&self) -> &[MemoryType] {
        &self.memories
    }
}

pub struct AssembledModule {
    module: Module,
    assembled: Vec<u8>,
}

impl Deref for AssembledModule {
    type Target = Module;

    fn deref(&self) -> &Self::Target {
        &self.module
    }
}

impl DerefMut for AssembledModule {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.module
    }
}

impl AssembledModule {
    pub fn binary(&self) -> &[u8] {
        &self.assembled
    }

    pub fn link_import(&mut self, module: &str, name: Option<&str>, addr: u64) {
        let relocation = self
            .imports
            .iter()
            .find_map(|(_, (module_, name_, offset))| {
                let names_equal = match (name, name_) {
                    (None, None) => false,
                    (None, Some(_)) => false,
                    (Some(_), None) => false,
                    (Some(name), Some(name_)) => name == name_,
                };
                if module_ == module && names_equal {
                    Some(*offset)
                } else {
                    None
                }
            });
        match relocation {
            Some(offset) => {
                let mut mem = &mut self.assembled[offset..offset + size_of::<u64>()];
                LittleEndian::write_u64(&mut mem, addr);
            }
            None => (),
        }
    }
}

impl Compiler for X86_64Compiler {
    type Error = Error;
    type Module = AssembledModule;

    fn compile(&self, module: &[u8]) -> Result<Self::Module, Self::Error> {
        let mut validator = Validator::default();
        validator.wasm_features(WasmFeatures {
            mutable_global: true,
            saturating_float_to_int: true,
            sign_extension: true,
            reference_types: true,
            multi_value: true,
            bulk_memory: true,
            module_linking: true,
            simd: true,
            relaxed_simd: true,
            threads: true,
            tail_call: true,
            deterministic_only: true,
            multi_memory: true,
            exceptions: true,
            memory64: true,
            extended_const: false,
        });
        let mut assembler = CodeAssembler::new(64)?;
        let mut got = BTreeMap::new();
        let mut ils = BTreeMap::new();
        let mut parser = wasmparser_nostd::Parser::new(0);
        let mut data: &[u8] = &module;
        let mut eof = false;
        let mut module = Module::new();
        let mut function_index = 0;
        let mut function_body_index = 0;
        let mut function_typedefs = BTreeMap::new();
        let mut function_type_index = 0;
        let mut function_types = BTreeMap::new();
        let mut label_indices = Vec::new();
        let mut function_bodies = Vec::new();
        loop {
            let parsed = parser.parse(&data, eof)?;

            match parsed {
                Chunk::Parsed { payload, consumed } => {
                    match payload {
                        Payload::End => {
                            validator.end()?;
                            break;
                        }
                        Payload::MemorySection(r) => {
                            validator.memory_section(&r)?;
                            for m in r {
                                let mem = m?;
                                module.memories.push(mem);
                            }
                        }
                        Payload::TypeSection(ts) => {
                            validator.type_section(&ts)?;
                            for t in ts {
                                let typedef = t?;
                                match typedef {
                                    TypeDef::Func(func_type) => {
                                        function_typedefs.insert(function_type_index, func_type);
                                        function_type_index += 1;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Payload::ImportSection(is) => {
                            validator.import_section(&is)?;
                            for i in is {
                                let import = i?;
                                let mut current_label = assembler.create_label();
                                assembler.set_label(&mut current_label)?;
                                assembler.zero_bytes()?;
                                let offset = assembler
                                    .assemble_options(
                                        0,
                                        BlockEncoderOptions::RETURN_NEW_INSTRUCTION_OFFSETS,
                                    )?
                                    .label_ip(&current_label)?
                                    as usize;
                                let reference = (
                                    import.module.to_owned(),
                                    import.field.map(str::to_owned),
                                    offset,
                                );
                                match import.ty {
                                    ImportSectionEntryType::Function(function_type) => {
                                        module.imports.insert(function_index, reference);
                                        let label = assembler.create_label();
                                        label_indices.push((assembler.instructions().len(), label));
                                        assembler.dq(&[0xBADC0FFEE0DDF00D])?;
                                        ils.insert(function_index, label);
                                        function_types.insert(function_index, function_type);
                                        function_index += 1;
                                        function_body_index += 1;
                                    }
                                    _ => (),
                                }
                            }
                        }
                        Payload::FunctionSection(fs) => {
                            validator.function_section(&fs)?;
                            for function_type in fs.into_iter() {
                                let label = assembler.create_label();
                                label_indices.push((assembler.instructions().len(), label));

                                let offset = assembler.instructions().len();
                                assembler.dq(&[0])?;
                                module.functions.insert(function_index, offset);
                                got.insert(function_index, assembler.create_label());
                                function_types.insert(function_index, function_type?);
                                function_index += 1;
                            }
                        }
                        Payload::ExportSection(es) => {
                            validator.export_section(&es)?;
                            for e in es.into_iter() {
                                let export = e?;
                                module
                                    .exports
                                    .insert(String::from(export.field), export.index);
                            }
                        }
                        Payload::CodeSectionEntry(cs) => {
                            let mut func_validator = validator.code_section_entry()?;
                            let function_type = function_types
                                .get(&function_body_index)
                                .and_then(|i| function_types.get(&i))
                                .and_then(|t| function_typedefs.get(t))
                                .cloned()
                                .unwrap();
                            let fun_label = got.get_mut(&function_body_index).unwrap();
                            function_bodies.push((*fun_label, function_body_index));
                            label_indices.push((assembler.instructions().len(), *fun_label));
                            let rd = cs.get_operators_reader()?;
                            assembler.push(rbp)?;
                            assembler.mov(rbp, rsp)?;
                            let mut integer_order: VecDeque<AsmRegister64> =
                                vec![rdi, rsi, rdx, rcx, r8, r9]
                                    .drain(0..function_type.params.len())
                                    .collect();

                            let mut locals = vec![];
                            let mut locals_size = 0;

                            for param in function_type.params.iter() {
                                let sz = param.encoding_size();
                                locals.push(locals_size + sz);
                                locals_size += sz;
                            }

                            for local in cs.get_locals_reader()?.into_iter() {
                                let offset = cs.get_binary_reader().current_position();
                                let (count, ty) = local?;
                                let sz = ty.encoding_size();
                                for i in 0..count {
                                    locals.push(locals_size + sz * i);
                                }
                                locals_size += sz * count;
                                func_validator.define_locals(offset, count, ty)?;
                            }

                            if locals_size > 0 {
                                // Allocate stack for locals
                                assembler.add_instruction(iced_x86::Instruction::with2(
                                    iced_x86::Code::Sub_rm64_imm32,
                                    iced_x86::Register::RSP,
                                    locals_size,
                                )?)?;
                            }

                            let mut extra_args_offset: u32 = 8; // past return address
                            for (index, param) in function_type.params.iter().enumerate() {
                                // We know that we have such a parameter in locals, no need to check
                                let i = *unsafe { locals.get_unchecked(index) };
                                match param {
                                    Type::I64 => match integer_order.pop_back() {
                                        Some(reg) => assembler.mov(ptr(rbp - i), reg)?,
                                        None => {
                                            assembler
                                                .mov(r11, qword_ptr(rbp + extra_args_offset))?;
                                            assembler.mov(ptr(rbp - i), r11)?;
                                            extra_args_offset += param.encoding_size();
                                        }
                                    },
                                    Type::I32 => match integer_order.pop_back() {
                                        Some(reg) => assembler.mov(ptr(rbp - i), reg)?,
                                        None => {
                                            assembler
                                                .mov(r11, dword_ptr(rbp + extra_args_offset))?;
                                            assembler.mov(ptr(rbp - i), r11)?;
                                            extra_args_offset += param.encoding_size();
                                        }
                                    },
                                    _ => todo!(),
                                }
                            }

                            let mut height = func_validator.operand_stack_height();

                            for op in rd.into_iter_with_offsets() {
                                let (op, offset) = op?;
                                func_validator.op(offset, &op)?;
                                height =
                                    std::cmp::max(height, func_validator.operand_stack_height());
                                instructions::handle_instruction(
                                    &mut assembler,
                                    &mut got,
                                    &mut ils,
                                    &mut function_typedefs,
                                    &mut function_types,
                                    &locals,
                                    op,
                                )?;
                            }

                            func_validator.finish(cs.get_binary_reader().current_position())?;

                            module
                                .function_stack_heights
                                .insert(function_body_index, height);

                            let mut integer_order = VecDeque::from([rax, rdx]);
                            for ret in function_type.returns.iter() {
                                match ret {
                                    Type::I64 | Type::I32 => match integer_order.pop_front() {
                                        Some(reg) => assembler.pop(reg)?,
                                        None => (),
                                    },
                                    _ => todo!(),
                                }
                            }

                            if locals_size > 0 {
                                // Deallocate stack for locals
                                assembler.add_instruction(iced_x86::Instruction::with2(
                                    iced_x86::Code::Add_rm64_imm32,
                                    iced_x86::Register::RSP,
                                    locals_size,
                                )?)?;
                            }

                            assembler.mov(rsp, rbp)?;
                            assembler.pop(rbp)?;
                            assembler.ret()?;
                            function_body_index += 1;
                        }
                        Payload::Version { num, range } => {
                            validator.version(num, &range)?;
                        }
                        Payload::AliasSection(a) => {
                            validator.alias_section(&a)?;
                        }
                        Payload::InstanceSection(i) => {
                            validator.instance_section(&i)?;
                        }
                        Payload::TableSection(t) => {
                            validator.table_section(&t)?;
                        }
                        Payload::TagSection(t) => {
                            validator.tag_section(&t)?;
                        }
                        Payload::GlobalSection(g) => {
                            validator.global_section(&g)?;
                        }
                        Payload::StartSection { func, range } => {
                            validator.start_section(func, &range)?;
                        }
                        Payload::ElementSection(e) => {
                            validator.element_section(&e)?;
                        }
                        Payload::DataCountSection { count, range } => {
                            validator.data_count_section(count, &range)?;
                        }
                        Payload::DataSection(d) => {
                            validator.data_section(&d)?;
                        }
                        Payload::CustomSection { .. } => {}
                        Payload::CodeSectionStart { count, range, .. } => {
                            validator.code_section_start(count, &range)?;
                        }
                        Payload::ModuleSectionStart { count, range, .. } => {
                            validator.module_section_start(count, &range)?;
                        }
                        Payload::ModuleSectionEntry { .. } => {
                            validator.module_section_entry();
                        }
                        Payload::UnknownSection { id, range, .. } => {
                            validator.unknown_section(id, &range)?;
                        }
                    }
                    data = &data[consumed..];
                    eof = data.len() == 0;
                }
                _ => (),
            }
        }
        // Optimize code
        for instruction in optimizer::optimize(assembler.take_instructions(), &mut label_indices)? {
            assembler.add_instruction(instruction)?;
        }
        // Bind labels
        for (idx, instruction) in assembler.take_instructions().into_iter().enumerate() {
            if let Some((_, label)) = label_indices.iter_mut().find(|(i, _)| *i == idx) {
                assembler.set_label(label)?;
                assembler.zero_bytes()?;
                // If this is a label pointing to a function, record function body entry point
                if let Some((_label, index)) =
                    function_bodies.iter().find(|(label_, _)| label_ == label)
                {
                    module.function_bodies.insert(
                        *index,
                        assembler
                            .assemble_options(
                                0,
                                BlockEncoderOptions::RETURN_NEW_INSTRUCTION_OFFSETS,
                            )?
                            .label_ip(label)? as usize,
                    );
                }
            }
            assembler.add_instruction(instruction)?;
        }
        Ok(module.assembled(assembler.assemble(0)?))
    }
}

#[cfg(feature = "test")]
impl AssembledModule {
    pub fn dump_asm(&self, offset: u64) {
        use iced_x86::{Formatter, Mnemonic};
        let first_function = self.function_bodies.values().min().cloned().unwrap_or(0);
        let binary = &self.binary()[first_function..];
        let decoder = iced_x86::Decoder::new(64, binary, iced_x86::DecoderOptions::NONE);
        let mut formatter = iced_x86::IntelFormatter::new();
        formatter.options_mut().set_uppercase_mnemonics(true);
        formatter.options_mut().set_rip_relative_addresses(true);
        for instr in decoder {
            if let Some((index, _)) = self
                .function_bodies
                .iter()
                .find(|(_, v)| (**v as u64 - first_function as u64) == instr.ip())
            {
                if let Some((name, _)) = self.exports.iter().find(|(_, v)| *v == index) {
                    println!("{}:", name);
                } else {
                    println!("{}:", index);
                }
            }
            let mut output = alloc::string::String::new();
            formatter.format(&instr, &mut output);
            print!("  {:016X} ", instr.ip() + offset + (first_function as u64));

            print!("{}", output);
            if instr.mnemonic() == Mnemonic::Call && instr.memory_displacement64() > 0 {
                print!(
                    " // -> {:016X}",
                    offset + instr.memory_displacement64() + first_function as u64
                );
            }

            print!(" ( ");
            let instr_bytes = &binary[instr.ip() as usize..instr.ip() as usize + instr.len()];
            for b in instr_bytes.iter() {
                print!("{:02X} ", b);
            }
            println!(")");
        }
    }
}
