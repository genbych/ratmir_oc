use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use crate::interrupts::{inb, outb, InterruptStackFrame};

struct Spinlock<T> {
    lock_: AtomicBool,
    buf: UnsafeCell<T>,
}

impl<T> Spinlock<T> {
    const fn new(data: T) -> Self {
        Self {
            lock_: AtomicBool::new(false),
            buf: UnsafeCell::new(data),
        }
    }
    fn lock(&self) {
        loop {
            if !self.lock_.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
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

    fn unlock(&self) {
        self.lock_.store(false, Ordering::Release);
    }

}

unsafe impl<T> Sync for Spinlock<T> where T: Send {}

struct KeyboardBuffer {
    buf: [u8; 256],
    head: u8,
    tail: u8
}
static KBD_BUFFER: Spinlock<KeyboardBuffer> = Spinlock::new(KeyboardBuffer {
    buf: [0; 256],
    head: 0,
    tail: 0
});

extern "x86-interrupt" fn keyboard_handler(frame: InterruptStackFrame, ) {

    unsafe {
        let scancode = inb(0x60);

        KBD_BUFFER.lock();

        let data = &mut *KBD_BUFFER.buf.get();

        data.buf[data.head as usize] = scancode;
        data.head = data.head.wrapping_add(1);

        KBD_BUFFER.unlock();

        outb(0x20, 0x20)
    }

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
        0x2B => Some('\'),
    }
}