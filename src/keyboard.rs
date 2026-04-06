use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use uefi::proto::console::text::Key::Printable;
use crate::interrupts::{inb, kernel_panic, outb, InterruptStackFrame};


pub struct Spinlock<T> {
    lock_: AtomicBool,
    pub buf: UnsafeCell<T>,
}

impl<T> Spinlock<T> {
    const fn new(data: T) -> Self {
        Self {
            lock_: AtomicBool::new(false),
            buf: UnsafeCell::new(data),
        }
    }
    pub fn lock(&self) {
        loop {
            if !self.lock_.swap(true, Ordering::Acquire) {
                return;
            }

            while self.lock_.load(Ordering::Relaxed) {
                core::hint::spin_loop();
            }
        }
    }
    fn try_lock(&self) -> bool {
        !self.lock_.load(Ordering::Relaxed) && !self.lock_.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok()

    }

    pub fn unlock(&self) {
        self.lock_.store(false, Ordering::Release);
    }

}

unsafe impl<T> Sync for Spinlock<T> where T: Send {}

pub struct KeyboardBuffer {
    buf: [u8; 256],
    head: u8,
    tail: u8
}

pub struct KeyboardState {
    pub shift: bool,
    pub caps: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub f1: bool,
    pub f2: bool,
    pub f3: bool,
    pub f4: bool,
    pub f5: bool,
    pub f6: bool,
    pub f7: bool,
    pub f8: bool,
    pub f9: bool,
    pub f10: bool,
    pub f11: bool,
    pub f12: bool,
    pub del: bool,
    pub u_arr: bool,
    pub d_arr: bool,
    pub l_arr: bool,
    pub r_arr: bool,
    pub esc: bool,
}

impl KeyboardState {
    pub const fn new() -> Self {
        Self{shift: false, caps: false, ctrl: false, alt: false, f1: false, f2: false, f3: false, f4: false, f5: false, f6: false, f7: false, f8: false, f9: false, f10: false, f11: false, f12: false, del: false, u_arr: false, d_arr: false, l_arr: false, r_arr: false, esc: false }
    }
}

impl KeyboardBuffer {
    pub fn get_scancode(&mut self) -> u8 {

        let scancode = unsafe {

            if self.head != self.tail {
                let code = self.buf[self.tail as usize];
                self.tail = self.tail.wrapping_add(1);

                code
            }
            else {
                0u8
            }
        };

        scancode

    }

    pub fn get_char(&mut self) -> Option<char> {
        let scancode = self.get_scancode();

        let char = scancode_to_char(scancode);

        char
    }
}

pub static KBD_BUFFER: Spinlock<KeyboardBuffer> = Spinlock::new(KeyboardBuffer {
    buf: [0; 256],
    head: 0,
    tail: 0
});
static mut IS_EXTENDED: bool = false;

pub static KBD_STATE: Spinlock<KeyboardState> = Spinlock::new(KeyboardState::new());

pub extern "x86-interrupt" fn keyboard_handler(frame: InterruptStackFrame, ) {

    unsafe {
        let scancode = inb(0x60);

        KBD_BUFFER.lock();

        let data = &mut *KBD_BUFFER.buf.get();

        if scancode == 0xE0 {
            IS_EXTENDED = true;

            KBD_BUFFER.unlock();
            outb(0x20, 0x20);
            return;
        }

        if IS_EXTENDED {
            extended_handler(Some(scancode));
            IS_EXTENDED = false;

            data.buf[data.head as usize] = scancode;
            data.head = data.head.wrapping_add(1);

        }

        else {
            data.buf[data.head as usize] = scancode;
            data.head = data.head.wrapping_add(1);
        }


        KBD_BUFFER.unlock();


        outb(0x20, 0x20)
    }

}

fn spec_handler(scancode: Option<u8>) -> Option<char> {
    if let Some(c) = scancode {
        KBD_STATE.lock();
        let state = unsafe {&mut *KBD_STATE.buf.get()};

        match c {
            0x3A => { state.caps = !state.caps ; }
            0x2A => { state.shift = true; }
            0x36 => {state.shift = true}
            0x1D => { state.ctrl = true; }
            0x38 => { state.alt = true; }
            0x01 => { state.esc = true; }
            0x3B => { state.f1 = true; }
            0x3C => { state.f2 = true; }
            0x3D => { state.f3 = true; }
            0x3E => { state.f4 = true; }
            0x3F => { state.f5 = true; }
            0x40 => { state.f6 = true; }
            0x41 => { state.f7 = true; }
            0x42 => { state.f8 = true; }
            0x43 => { state.f9 = true; }
            0x44 => { state.f10 = true; }
            0x57 => { state.f11 = true; }
            0x58 => { state.f12 = true; }
            0xAA => { state.shift = false; }
            0xB6 => { state.shift = false}
            0x9D => { state.ctrl = false; }
            0xB8 => { state.alt = false; }
            0x81 => { state.esc = false; }
            0xBB => { state.f1 = false; }
            0xBC => { state.f2 = false; }
            0xBD => { state.f3 = false; }
            0xBE => { state.f4 = false; }
            0xBF => { state.f5 = false; }
            0xC0 => { state.f6 = false; }
            0xC1 => { state.f7 = false; }
            0xC2 => { state.f8 = false; }
            0xC3 => { state.f9 = false; }
            0xC4 => { state.f10 = false; }
            0xD7 => { state.f11 = false; }
            0xD8 => { state.f12 = false; }
            _ => {}

        }

        KBD_STATE.unlock();

    }
    None
}

fn extended_handler(scancode: Option<u8>) -> Option<char> {
    if let Some(c) = scancode {
        KBD_STATE.lock();
        let state = unsafe {&mut *KBD_STATE.buf.get()};

        match c {
            0x53 => { state.del = true; }
            0x48 => { state.u_arr = true; }
            0x50 => { state.d_arr = true; }
            0x4B => { state.l_arr = true; }
            0x4D => { state.r_arr = true; }
            _ => {}
        }
    }

    KBD_STATE.unlock();
    None
}

pub fn scancode_to_char(scancode: u8) -> Option<char> {
    match scancode {
        0x1E => Some('A'),
        0x30 => Some('B'),
        0x2E => Some('C'),
        0x20 => Some('D'),
        0x12 => Some('E'),
        0x21 => Some('F'),
        0x22 => Some('G'),
        0x23 => Some('H'),
        0x17 => Some('I'),
        0x24 => Some('J'),
        0x25 => Some('K'),
        0x26 => Some('L'),
        0x32 => Some('M'),
        0x31 => Some('N'),
        0x18 => Some('O'),
        0x19 => Some('P'),
        0x10 => Some('Q'),
        0x13 => Some('R'),
        0x1F => Some('S'),
        0x14 => Some('T'),
        0x16 => Some('U'),
        0x2F => Some('V'),
        0x11 => Some('W'),
        0x2D => Some('X'),
        0x15 => Some('Y'),
        0x2C => Some('Z'),
        0x0B => Some('0'),
        0x02 => Some('1'),
        0x03 => Some('2'),
        0x04 => Some('3'),
        0x05 => Some('4'),
        0x06 => Some('5'),
        0x07 => Some('6'),
        0x08 => Some('7'),
        0x09 => Some('8'),
        0x0A => Some('9'),
        0x29 => Some('`'),
        0x0C => Some('-'),
        0x0D => Some('='),
        0x2B => Some('\\'),
        0x0E => Some('\u{8}'),
        0x39 => Some(' '),
        0x0F => Some('\t'),
        0x1C => Some('\n'),
        0x1A => Some('['),
        0x1B => Some(']'),
        0x27 => Some(';'),
        0x28 => Some('\''),
        0x33 => Some(','),
        0x34 => Some('.'),
        0x35 => Some('/'),
        _ => { spec_handler(Some(scancode)) }
    }
}