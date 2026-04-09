
use core::fmt;
use crate::display::{get_char_data, Display};
use crate::interrupts::kernel_panic;
use crate::keyboard::{Spinlock, KBD_BUFFER};
use crate::PANIC_DISPLAY;

pub struct TextGrid {
    pub rows: usize,
    pub cols: usize,
}

pub struct Console<'a> {
    pub display: &'a Display,
    pub grid: TextGrid,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub line_lengths: [usize; 256],
    pub foreground: u32,
    pub background: u32,
}
pub struct Stack<T, const N: usize> {
    data: [Option<T>; N],
    top: usize,
}

#[derive(Copy, Clone)]
pub struct Buffer<const N: usize> {
    pub data: [u8; N],
    pub cursor: usize,
}

impl<const N: usize> Buffer<N> {
    pub const fn new() -> Self {
        Self { data: [0u8; N], cursor: 0 }
    }

    pub fn clear(&mut self) {
        self.data.fill(0u8);
        self.cursor = 0;
    }
    pub fn push(&mut self, ch: u8) {
        self.data[self.cursor] = ch;
        self.cursor += 1;
    }

    pub fn pop(&mut self) -> Option<u8> {
        if self.cursor > 0 {
            self.data[self.cursor] = 0u8;
            self.cursor -= 1;
        }
        Some(self.data[self.cursor])
    }


}

pub static BUFFER: Spinlock<Buffer::<256>> =  Spinlock::new( Buffer::<256>::new() );
pub static STACK: Spinlock<Stack::<Buffer<256>, 256>> = Spinlock::new(Stack {
    data: [const { Some(Buffer::<256>::new()) }; 256],
    top: 0,
});


impl<T, const N: usize> Stack<T, N> {

    pub fn push(&mut self, data: T) {
        if self.top < N {
            self.data[self.top] = Some(data);
            self.top += 1;
        }
        else {
            kernel_panic("Stack Overflow");
        }
    }
    pub fn pop(&mut self) -> Option<T> {
        if self.top > 0 {
            self.top -= 1;
            self.data[self.top].take()
        }
        else {
            None
        }
    }
}

impl<'a> Console<'a> {

    pub fn scroll(&mut self) {

        STACK.lock();
        BUFFER.lock();
        let mut buf = unsafe { *BUFFER.buf.get() };
        let st = unsafe { &mut *STACK.buf.get() };

        st.push(buf);
        buf.clear();

        STACK.unlock();
        BUFFER.unlock();


        unsafe {
            let bytes_per_pixel = 4;
            let one_row_bytes = self.display.stride * 16 * bytes_per_pixel;
            let total_rows_to_move = self.display.height - 16;
            let total_bytes_to_move = self.display.stride * total_rows_to_move * bytes_per_pixel;


            let dst = self.display.back_ptr;
            let src = self.display.back_ptr.add(self.display.stride * 16);

            core::ptr::copy(src, dst, self.display.stride * total_rows_to_move);
        }

        let last_line = (self.display.height / 16) - 1;
        self.display.clear_line(last_line);

        self.display.update();
    }

    pub fn check_scroll(&mut self) {
        if self.cursor_y >= self.grid.rows {
            self.scroll();
            self.cursor_y = self.grid.rows - 1;
        }
    }

    pub fn backspace(&mut self) {
        if self.cursor_x > 0 {
            self.cursor_x -= 1;
            BUFFER.lock();
            let mut buf = unsafe { *BUFFER.buf.get() };

            buf.pop();

            BUFFER.unlock();
        }
        else if self.cursor_y > 0 {
            self.cursor_y -= 1;
            self.cursor_x = self.line_lengths[self.cursor_y];

            BUFFER.lock();

            let mut buf = unsafe { *BUFFER.buf.get() };
            buf.pop();

            BUFFER.unlock();
        }
        else {
            return;
        }

        let x = self.cursor_x * 8;
        let y = self.cursor_y * 16;

        self.display.rect(x, y, 8, 16, self.background);

        self.display.update_region(x, y, 8, 16)
    }

    pub fn write_char(&mut self, c: char, push: bool) {

        let mut skip = false;

        match c {
            '\n' => {
                self.line_lengths[self.cursor_y] = self.cursor_x;
                self.cursor_x = 0;
                self.cursor_y += 1;
                self.check_scroll();
                skip = true;
            }

            _ => {}
        }

        let char = get_char_data(c);
        let start_x = self.cursor_x * 8;
        let start_y = self.cursor_y * 16;


        if !skip{
            if push {
                BUFFER.lock();
                let buf = unsafe {&mut *BUFFER.buf.get()};

                buf.push(c as u8);

                BUFFER.unlock();
            }

            for row in 0..char.len() {
                let row_data = char[row];

                for bit in 0..8 {
                    let px = start_x + bit;
                    let py = start_y + row;

                    let color = if (row_data & (0x80 >> bit)) != 0 {
                        self.foreground
                    } else {
                        self.background
                    };

                    self.display.write_pixel( px, py, color);
                }
            }

            self.cursor_x += 1;

            self.line_lengths[self.cursor_y] = self.cursor_x;

            if self.cursor_x > self.grid.cols {
                self.cursor_x = 0;
                self.cursor_y += 1;
            }
        }


        self.display.update_region(start_x, start_y, 8, 16);

    }
    pub fn command_handler(&self, buf: &'a [u8; 256]) -> (Option<&'a str>, core::str::SplitWhitespace<'a>)  {

        let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());


        let clean_buf = &buf[..len];

        let s = core::str::from_utf8(clean_buf).unwrap_or("");

        let mut words = s.split_whitespace();
        let command = words.next();
        let args = words;


        (command, args)

    }
}

impl<'a> fmt::Write for Console<'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            self.write_char(c, false);
        }
        Ok(())
    }
}
