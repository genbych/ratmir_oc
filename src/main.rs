#![no_std]
#![no_main]

use core::panic::PanicInfo;
use uefi::prelude::*;
use uefi::println;
use uefi::proto::console::gop::GraphicsOutput;
use uefi::table::boot::MemoryType;
use core::fmt;
use core::fmt::Write;

struct Display {
    ptr: *mut u32,
    width: usize,
    height: usize,
    stride: usize,
}

struct Console<'a> {
    display: &'a Display,
    grid: TextGrid,
    cursor_x: usize,
    cursor_y: usize,
    foreground: u32,
    background: u32,
}

struct TextGrid {
    rows: usize,
    cols: usize,
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

    st.stdin().reset(false).unwrap();

    let display = Display {
        ptr: ptr,
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
        foreground: 0xFFFFFFFF,
        background: 0x00000000,

    };

    memory_size /= 2_u64.pow(20);

    display.rect(0, 0, display.width, display.height, 0x00000000);
    write!(console, "Welcome to RatmirOC\nMemory available: {} MB", memory_size).expect("error to write");



    let key_event = st
        .stdin()
        .wait_for_key_event()
        .expect("Keyboard event not available");

    st.boot_services()
        .wait_for_event(&mut [key_event])
        .discard_errdata();

    Status::SUCCESS
}


impl Display {
    fn draw_pixel(&self, x: usize, y: usize, color: u32) {
        unsafe {
            if x > self.width || y > self.height {
                return;
            }

            self
                .ptr
                .add(y * self.stride + x)
                .write_volatile(color);
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
                self.draw_pixel( i, j, color)
            }
        }
    }
}

impl<'a> Console<'a> {
    fn write_char(&mut self, c: char) {

        match c {
            '\n' => {
                self.cursor_x = 0;
                self.cursor_y += 1;
                return;
            }

            '\r' => {
                self.cursor_x = 0;
                return;
            }
            _ => {}
        }

        let char = get_char_data(c);
        let start_x = self.cursor_x * 8;
        let start_y = self.cursor_y * 16;

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

                self.display.draw_pixel( px, py, color);
            }
        }

        self.cursor_x += 1;

        if self.cursor_x > self.grid.cols {
            self.cursor_x = 0;
            self.cursor_y += 1;
        }

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
