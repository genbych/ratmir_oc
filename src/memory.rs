use uefi::table::boot::MemoryType;
use crate::interrupts::kernel_panic;
use crate::keyboard::Spinlock;

pub struct BitmapAllocator {
    pub bitmap: *mut u8,
    pub total_pages: usize,
    pub last_found_idx: usize,
}

unsafe impl Send for BitmapAllocator {}
unsafe impl Sync for BitmapAllocator {}
pub static ALLOCATOR: Spinlock<Option<BitmapAllocator>> = Spinlock::new(None);

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