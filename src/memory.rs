use uefi::table::boot::MemoryType;
use crate::interrupts::kernel_panic;
use crate::keyboard::Spinlock;

pub struct BitmapAllocator {
    pub bitmap: *mut u8,
    pub total_pages: usize,
    pub last_found_idx: usize,
}
pub struct PageTable {
    data: [PageTableEntry; 512],
}

#[derive(Copy, Clone)]
pub struct PageTableEntry {
    addr: u64,
}

pub struct VirtAddr {
    addr: u64,
}

pub struct Flags;

impl Flags {
    pub const PRESENT: u64 = 1 << 0;
    pub const WRITABLE: u64 = 1 << 1;
    pub const USER: u64 = 1 << 2;
}

unsafe impl Send for BitmapAllocator {}
unsafe impl Sync for BitmapAllocator {}
pub static ALLOCATOR: Spinlock<Option<BitmapAllocator>> = Spinlock::new(None);


impl VirtAddr {
    pub fn new(addr: u64) -> Self {
        Self { addr: addr }
    }

    pub fn pml4_index(&self) -> u64 {
        let mask = 0x1FF;
        let mut addr = self.addr;

        addr = addr >> 39;

        addr & mask
    }
    pub fn pml3_index(&self) -> u64 {
        let mask = 0x1FF;
        let mut addr = self.addr;

        addr = addr >> 30;

        addr & mask
    }
    pub fn pml2_index(&self) -> u64 {
        let mask = 0x1FF;
        let mut addr = self.addr;

        addr = addr >> 21;

        addr & mask
    }
    pub fn pml1_index(&self) -> u64 {
        let mask = 0x1FF;
        let mut addr = self.addr;

        addr = addr >> 12;

        addr & mask
    }
}

impl PageTable {
    pub fn new() -> Self {
        Self {data: [PageTableEntry { addr: 0 }; 512]}
    }

    pub fn clear(&mut self) {
        self.data = [PageTableEntry { addr: 0 }; 512];
    }
}

impl PageTableEntry {
    pub fn set_addr(&mut self, addr: u64, flags: u64) {
        let mask = 0x000F_FFFF_FFFF_F000;

        self.addr = addr & mask;
        let flags = flags & !mask;

        self.addr |= flags;
    }
    pub fn get_addr(&self) -> u64 {
        let mask = 0x000F_FFFF_FFFF_F000;

        self.addr & mask
    }

}
impl BitmapAllocator {
    pub fn new(mmap: uefi::table::boot::MemoryMap) -> Self {
        let mut page_count = 0;
        for descriptor in mmap.entries() {
            page_count += descriptor.page_count;
        }

        let bytes_needed = (page_count as usize + 7) / 8;
        let mut bitmap_ptr: *mut u8 = core::ptr::null_mut();

        for descriptor in mmap.entries() {
            if descriptor.ty == MemoryType::CONVENTIONAL && (descriptor.page_count * 4096) as usize >= bytes_needed {
                if descriptor.phys_start >= 0x100000 {
                    bitmap_ptr = descriptor.phys_start as *mut u8;
                    break;
                }
            }
        }

        if bitmap_ptr.is_null() { kernel_panic("No memory available"); }

        unsafe { core::ptr::write_bytes(bitmap_ptr, 0xFF, bytes_needed) };

        let mut allocator = Self {
            bitmap: bitmap_ptr,
            total_pages: page_count as usize,
            last_found_idx: 0
        };

        for descriptor in mmap.entries() {
            if descriptor.ty == MemoryType::CONVENTIONAL {
                allocator.free(descriptor.phys_start as usize, descriptor.page_count as usize);
            }
        }

        allocator.lock(bitmap_ptr as usize, (bytes_needed + 4095) / 4096);
        allocator.lock(0, 1);

        allocator
    }
    pub fn set_bit(&mut self, index: usize, value: bool) {
        if index >= self.total_pages { return; }


        let byte_offset = index / 8;
        let bit_offset = index % 8;

        let byte_ptr = unsafe { self.bitmap.add(byte_offset) };

        let mut byte = unsafe { core::ptr::read_volatile(byte_ptr) };

        if value {
            byte |= 1 << bit_offset;
        }
        else {
            byte &= !(1 << bit_offset);
        }

        unsafe { core::ptr::write_volatile(byte_ptr, byte ); }
    }
    pub fn free(&mut self, addr: usize, count: usize) {
        let start_idx = addr / 4096;
        for idx in 0..count {
            self.set_bit(start_idx + idx, false)
        }
    }

    pub fn lock(&mut self, addr: usize, count: usize) {
        let start_idx = addr / 4096;
        for idx in 0..count {
            self.set_bit(start_idx + idx, true)
        }
    }

    pub fn get_bit(&self, index: usize) -> bool {
        let byte_offset = index / 8;
        let bit_offset = index % 8;

        let byte_ptr = unsafe { self.bitmap.add(byte_offset) };
        let byte = unsafe { core::ptr::read_volatile(byte_ptr) };

        (byte & (1 << bit_offset)) != 0
    }

    pub fn alloc(&mut self) -> u64 {
        for idx in self.last_found_idx..self.total_pages {
            if !self.get_bit(idx) {
                self.last_found_idx = idx;
                self.lock(idx * 4096, 1);
                return (idx * 4096) as u64
            }
        }
        0u64
    }
}

pub fn map_page(virt: &mut VirtAddr, phys: u64, flags: u64, plm4: *mut PageTable) {
    let levels = [virt.pml4_index(), virt.pml3_index(), virt.pml2_index(), virt.pml1_index()];
    let mut current_table = plm4;

    for (i, &level) in levels.iter().enumerate() {
        let entry = unsafe { &mut (*current_table).data[level as usize] };

        if i == 3 {
            entry.set_addr(phys, flags);
            return;
        }

        if (entry.addr & (Flags::PRESENT as u64)) == 0 {
            ALLOCATOR.lock();

            let alloc = unsafe {&mut *ALLOCATOR.buf.get() };
            let addr =alloc.as_mut().unwrap().alloc();

            ALLOCATOR.unlock();

            let ptr = addr as *mut PageTable;

            unsafe {
                (*ptr).clear();
            }

            entry.set_addr(addr, (Flags::PRESENT as u64) | (Flags::WRITABLE as u64));

        }

        current_table = entry.get_addr() as *mut PageTable;

    }
}