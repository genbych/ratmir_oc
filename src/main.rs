#![no_std]
#![no_main]

use core::panic::PanicInfo;
use uefi::prelude::*;
use uefi::println;
use uefi::proto::console::gop::GraphicsOutput;
use uefi::table::boot::MemoryType;
use core::fmt;

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

    rect(&display, 0, 0, display.width, display.height, 0x00000000);
    print(&mut console, "Welcome to RatmirOC\nThis is my first OC");



    let key_event = st
        .stdin()
        .wait_for_key_event()
        .expect("Keyboard event not available");

    st.boot_services()
        .wait_for_event(&mut [key_event])
        .discard_errdata();

    Status::SUCCESS
}

fn draw_pixel(display: &Display, x: usize, y: usize, color: u32) {
    unsafe {
        if x > display.width || y > display.height {
            return;
        }

        display
            .ptr
            .add(y * display.stride + x)
            .write_volatile(color);
    }
}

fn rect(display: &Display, x: usize, y: usize, mut w: usize, mut h: usize, color: u32) {
    if x.checked_add(w).expect("overflow") > display.width {
        w = display.width - x;
    }
    if y.checked_add(h).expect("overflow") > display.height {
        h = display.height - y;
    }

    for i in x..x.checked_add(w).expect("overflow") {
        for j in y..y.checked_add(h).expect("overflow") {
            draw_pixel(display, i, j, color)
        }
    }
}

impl Display {
    fn draw_pixel(display: &Display, x: usize, y: usize, color: u32) {
        unsafe {
            if x > display.width || y > display.height {
                return;
            }

            display
                .ptr
                .add(y * display.stride + x)
                .write_volatile(color);
        }
    }

    fn rect(display: &Display, x: usize, y: usize, mut w: usize, mut h: usize, color: u32) {
        if x.checked_add(w).expect("overflow") > display.width {
            w = display.width - x;
        }
        if y.checked_add(h).expect("overflow") > display.height {
            h = display.height - y;
        }

        for i in x..x.checked_add(w).expect("overflow") {
            for j in y..y.checked_add(h).expect("overflow") {
                draw_pixel(display, i, j, color)
            }
        }
    }
}

fn draw_char(console: &mut Console, c: char) {

    match c {
        '\n' => {
            console.cursor_x = 0;
            console.cursor_y += 1;
            return;
        }

        '\r' => {
            console.cursor_x = 0;
            return;
        }
        _ => {}
    }

    let char = get_char_data(c);
    let start_x = console.cursor_x * 8;
    let start_y = console.cursor_y * 16;

    for row in 0..char.len() {
        let row_data = char[row];

        for bit in 0..8 {
            let px = start_x + bit;
            let py = start_y + row;

            let color = if (row_data & (0x80 >> bit)) != 0 {
                console.foreground
            } else {
                console.background
            };

            draw_pixel(console.display, px, py, color);
        }
    }

    console.cursor_x += 1;

    if console.cursor_x > console.grid.cols {
        console.cursor_x = 0;
        console.cursor_y += 1;
    }

}

fn print(console: &mut Console, word: &str, ) {
    for char in word.chars() {
        draw_char(console, char)
    }
}

const FONT: &[u8] = include_bytes!("IBM_VGA_8x16.bin");

fn get_char_data<'a>(c: char) -> &'a[u8] {
    let code = c as usize;

    let start = code * 16;
    let end = start + 16;

    &FONT[start..end]
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
