#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

use core::panic::PanicInfo;
use uefi::prelude::*;
use uefi::{println, Char16};
use uefi::proto::console::gop::GraphicsOutput;
use uefi::table::boot::MemoryType;
use core::fmt;
use core::fmt::Write;
use uefi::proto::console::text::{Key, ScanCode};

static mut ERROR_CONSOLE: Option<Console> = None;

#[repr(C, packed)]
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

struct Display {
    ptr: *mut u32,
    back_ptr: *mut u32,
    width: usize,
    height: usize,
    stride: usize,
}

struct Console<'a> {
    display: &'a Display,
    grid: TextGrid,
    cursor_x: usize,
    cursor_y: usize,
    line_lengths: [usize; 256],
    foreground: u32,
    background: u32,
}

struct TextGrid {
    rows: usize,
    cols: usize,
}

extern "x86-interrupt" fn divide_error(frame: InterruptStackFrame) {


}

#[entry]
fn main(handle: Handle, mut st: SystemTable<Boot>) -> Status {
    uefi::helpers::init(&mut st).unwrap();


    let (ptr, stride, resolution) = {
        let bs = st.boot_services();

        let gop_handle = bs
            .get_handle_for_protocol::<GraphicsOutput>()
            .expect("GOP handle not found");
        let mut gop = bs
            .open_protocol_exclusive::<GraphicsOutput>(gop_handle)
            .expect("Failed to open GOP");

        let mut fb = gop.frame_buffer();
        let ptr = fb.as_mut_ptr() as *mut u32;
        let mode_info = gop.current_mode_info();

        (ptr, mode_info.stride(), mode_info.resolution())
    };

    let bs = st.boot_services();

    let mmap = bs.memory_map(MemoryType::LOADER_DATA).unwrap();

    let mut memory_size: u64 = 0;

    for descriptor in mmap.entries() {
        if descriptor.ty == MemoryType::CONVENTIONAL {
            memory_size += descriptor.page_count * 4096;
        }
    }

    let back_ptr = bs.allocate_pool(MemoryType::LOADER_DATA, stride * resolution.1 * 4).unwrap().as_ptr() as *mut u32;

    st.stdin().reset(false).unwrap();

    let display = Display {
        ptr: ptr,
        back_ptr: back_ptr,
        width: resolution.0,
        height: resolution.1,
        stride: stride,
    };

    let grid = TextGrid {
        rows: resolution.1 / 16 ,
        cols: resolution.0 / 8,
    };

    let mut console = Console {
        display: &display,
        grid: grid,
        cursor_x: 0,
        cursor_y: 0,
        line_lengths: [0; 256],
        foreground: 0xFFFFFFFF,
        background: 0x00000000,

    };



    memory_size /= 2_u64.pow(20);
    display.rect(0, 0, display.width, display.height, 0x00000000);
    let space_key = Char16::try_from('\r').unwrap();
    let backspace_key = Char16::try_from('\u{8}').unwrap();

    for i in 0..200 {
        write!(&mut console, "Scrolling line {}\n", i).unwrap();
    }


    loop {

        let line_heights = console.line_lengths;


        let key_event = st
            .stdin()
            .wait_for_key_event()
            .expect("Keyboard event not available");

        st.boot_services()
            .wait_for_event(&mut [key_event])
            .discard_errdata();


        if let Some(key_event) = st.stdin().read_key().unwrap() {
            match key_event {
                Key::Printable(key) if key == space_key => {
                    console.write_char('\n');
                }

                Key::Printable(key) if key == backspace_key => {
                    console.backspace();
                }

                Key::Printable(c) => {
                    let ch: char = c.into();
                    console.write_char(ch);
                }

                Key::Special(ScanCode::ESCAPE) => {
                    break;
                }


                _ => {}
            }
        }
    }
    Status::SUCCESS

}


impl Display {
    fn write_pixel(&self, x: usize, y: usize, color: u32) {
        unsafe {
            if x > self.width || y > self.height {
                return;
            }

            self
                .back_ptr
                .add(y * self.stride + x)
                .write_volatile(color);
        }
    }

    fn update(&self) {
        unsafe {
            core::ptr::copy_nonoverlapping(self.back_ptr, self.ptr, self.stride * self.height)
        }
    }

    fn update_region(&self, x: usize, y: usize, w: usize, h: usize) {
        unsafe {
            for row in y..(y+h) {
                let offset = self.stride * row + x;
                let src = self.back_ptr.add(offset);
                let dst = self.ptr.add(offset);

                core::ptr::copy_nonoverlapping(src, dst, w);
            }
        }
    }



    fn rect(&self, x: usize, y: usize, mut w: usize, mut h: usize, color: u32) {
        if x.checked_add(w).expect("overflow") > self.width {
            w = self.width - x;
        }
        if y.checked_add(h).expect("overflow") > self.height {
            h = self.height - y;
        }

        for i in x..x.checked_add(w).expect("overflow") {
            for j in y..y.checked_add(h).expect("overflow") {
                self.write_pixel( i, j, color)
            }
        }
    }

    fn clear_line(&self, line_index: usize) {
        unsafe {
            let offset = line_index * 16 * self.stride;
            let line_ptr = self.back_ptr.add(offset);

            let count = self.stride * 16;

            core::ptr::write_bytes(line_ptr, 0, count);
        }
    }
}

impl<'a> Console<'a> {

    fn scroll(&mut self) {

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

    fn check_scroll(&mut self) {
        if self.cursor_y >= self.grid.rows {
            self.scroll();
            self.cursor_y = self.grid.rows - 1;
        }
    }

    fn backspace(&mut self) {
        if self.cursor_x > 0 {
            self.cursor_x -= 1;
        }
        else if self.cursor_y > 0 {
            self.cursor_y -= 1;
            self.cursor_x = self.line_lengths[self.cursor_y];
        }
        else {
            return;
        }

        let x = self.cursor_x * 8;
        let y = self.cursor_y * 16;

        self.display.rect(x, y, 8, 16, self.background);

        self.display.update_region(self.cursor_x, self.cursor_y, 8, 16)
    }

    fn write_char(&mut self, c: char) {

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


        if !skip {
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
}

const FONT: &[u8] = include_bytes!("IBM_VGA_8x16.bin");

fn get_char_data<'a>(c: char) -> &'a[u8] {
    let code = c as usize;

    let start = code * 16;
    let end = start + 16;

    &FONT[start..end]
}

impl<'a> fmt::Write for Console<'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            self.write_char(c);
        }
        Ok(())
    }
}


#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
