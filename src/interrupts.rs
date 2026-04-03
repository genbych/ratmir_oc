use crate::display::Display;
use crate::PANIC_DISPLAY;
#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    types: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

#[repr(C)]
pub struct InterruptStackFrame {
    pub instruction_pointer: u64,
    pub code_segment: u64,
    pub cpu_flags: u64,
    pub stack_pointer: u64,
    pub stack_segment: u64,
}

#[repr(C, align(16))]
pub struct Idt([IdtEntry; 256]);

#[repr(C, packed)]
pub struct Idtr {
    limit: u16,
    base: u64,
}

impl Idtr {
    pub fn new(idt_ptr: *const Idt) -> Self {
        Self {
            limit: (core::mem::size_of::<Idt>() - 1) as u16,
            base: idt_ptr as u64,
        }
    }
}

impl IdtEntry {
    pub const fn missing() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            types: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }
}

impl Idt {
    pub const fn new() -> Self {
        Self([IdtEntry::missing(); 256])
    }

    pub fn init(&mut self) {
        self.set_handles(0, divide_error as u64);
        self.set_handles(1, debug_handler as u64);
        self.set_handles(8, double_fault as u64);
        self.set_handles(13, general_protection_fault as u64);
        self.set_handles(14, page_fault as u64);
    }

    pub fn set_handles(&mut self, index: u8, handler: u64) {
        let low = handler as u16;
        let mid = (handler >> 16) as u16;
        let high = (handler >> 32) as u32;
        let selector = get_cs();
        let ist = 0x00;
        let types = 0x8E;

        self.0[index as usize] = IdtEntry {
            offset_low: low,
            selector: selector,
            ist: ist,
            types: types,
            offset_mid: mid,
            offset_high: high,
            reserved: 0,
        }
    }
}

pub fn kernel_panic(reason: &str) {
    unsafe {
        if let Some(ref disp) = PANIC_DISPLAY {
            disp.rect(0, 0, disp.width, disp.height, 0x14452F);
            disp.write_str_at("KERNEL PANIC", disp.width / 2 - 16, 50, 0xFFFFFFFF);
            disp.write_str_at(reason, disp.width / 2 - 16, 70, 0xFFFFFFFF);

            disp.update();
        }
    }
    loop {}
}

extern "x86-interrupt" fn divide_error(frame: InterruptStackFrame) {
    kernel_panic("Divide-Error");

    loop {}
}

extern "x86-interrupt" fn double_fault(frame: InterruptStackFrame, error_code: u64) {
    kernel_panic("Double-Fault");

    loop {}
}
extern "x86-interrupt" fn general_protection_fault(frame: InterruptStackFrame, error_code: u64) {
    kernel_panic("General-Protection-Fault");

    loop {}
}
extern "x86-interrupt" fn page_fault(frame: InterruptStackFrame, error_code: u64) {
    kernel_panic("Page-Fault");

    loop {}
}
extern "x86-interrupt" fn debug_handler(frame: InterruptStackFrame) {
    unsafe {
        if let Some(ref d) = PANIC_DISPLAY {
            d.rect(0, 0, 300, 50, 0xFFFF00);
            d.write_str_at("DEBUG STEP HIT", 10, 10, 0x000000);
            d.update();
        }
    }
}

fn get_cs() -> u16 {
    let cs: u16;
    unsafe {
        core::arch::asm!("mov {0:x}, cs", out(reg) cs);
    }
    cs
}





