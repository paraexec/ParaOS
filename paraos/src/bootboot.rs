use core::mem::size_of;

#[repr(C)]
pub union Arch {
    pub x86_64: X86_64,
    pub aarch64: Aarch64,
    _union_align: [u64; 8usize],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct X86_64 {
    pub acpi_ptr: u64,
    pub smbi_ptr: u64,
    pub efi_ptr: u64,
    pub mp_ptr: u64,
    pub unused0: u64,
    pub unused1: u64,
    pub unused2: u64,
    pub unused3: u64,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Aarch64 {
    pub acpi_ptr: u64,
    pub mmio_ptr: u64,
    pub efi_ptr: u64,
    pub unused0: u64,
    pub unused1: u64,
    pub unused2: u64,
    pub unused3: u64,
    pub unused4: u64,
}

#[repr(C, packed)]
pub struct MemoryMapping {
    _ptr: u64,
    _size: u64,
}

impl MemoryMapping {
    #[allow(dead_code)]
    pub fn addr(&self) -> usize {
        self._ptr as usize
    }
    #[allow(dead_code)]
    pub fn size(&self) -> usize {
        (self._size & 0xFFFFFFFFFFFFFFF0) as usize
    }
    #[allow(dead_code)]
    pub fn is_free(&self) -> bool {
        (self._size & 0xf) == 1
    }
}

#[repr(C, packed)]
pub struct Bootboot {
    pub magic: [u8; 4],
    pub size: u32,
    pub protocol: u8,
    pub framebuffer_type: u8,
    pub numcores: u16,
    pub bspid: u16,
    pub timezone: i16,
    pub datetime: [u8; 8usize],
    pub initrd_ptr: u64,
    pub initrd_size: u64,
    pub fb_ptr: *mut u8,
    pub fb_size: u32,
    pub fb_width: u32,
    pub fb_height: u32,
    pub fb_scanline: u32,
    pub arch: Arch,
    mmap: [MemoryMapping; (0xfff - 0x80) / size_of::<MemoryMapping>()],
}

impl Bootboot {
    #[allow(dead_code)]
    pub fn memory_mappings(&self) -> &[MemoryMapping] {
        &self.mmap[0..((self.size - 128) / 16) as usize]
    }
}
