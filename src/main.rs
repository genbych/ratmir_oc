
#![no_std]
#![no_main]

use core::panic::PanicInfo;
use uefi::prelude::*;
use uefi::println;
use uefi::table::boot::{ MemoryType };
use uefi::proto::console::gop::GraphicsOutput;

struct Display {
    ptr: *mut u32,
    width: usize,
    height: usize,
    stride: usize,
}

#[entry]
fn main(handle: Handle, mut st: SystemTable<Boot>) -> Status{
    uefi::helpers::init(&mut st).unwrap();


    let (ptr, stride, resolution) = {
        let bs = st.boot_services();

        let gop_handle = bs.get_handle_for_protocol::<GraphicsOutput>().expect("GOP handle not found");
        let mut gop = bs.open_protocol_exclusive::<GraphicsOutput>(gop_handle).expect("Failed to open GOP");

        let mut fb = gop.frame_buffer();
        let ptr = fb.as_mut_ptr() as *mut u32;
        let mode_info = gop.current_mode_info();

        (ptr, mode_info.stride(), mode_info.resolution())
    };


    let bs = st.boot_services();

    let mmap = bs.memory_map(MemoryType::LOADER_DATA) .unwrap();

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

    rect(&display, 100, 100, 100, 10000, 0x00FF00FF);

    let key_event = st.stdin().wait_for_key_event().expect("Keyboard event not available");

    st.boot_services()

        .wait_for_event(&mut [key_event])

        .discard_errdata();


    Status::SUCCESS
}

fn draw_pixel(display:  &Display, x: usize, y: usize, color: u32) {
    unsafe {
        if x > display.width || y > display.height {
            return;
        }

        display.ptr.add(y * display.stride + x).write_volatile(color);
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

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

