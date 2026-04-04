#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

mod display;
mod console;
mod interrupts;
mod keyboard;

use core::panic::PanicInfo;
use uefi::prelude::*;
use uefi::{Char16};
use uefi::proto::console::gop::GraphicsOutput;
use uefi::table::boot::MemoryType;
use core::fmt::Write;
use uefi::proto::console::text::{Key, ScanCode};
use display::{Display};
use console::{Console, TextGrid};
use interrupts::{kernel_panic, Idtr, Idt, IdtEntry};

pub static mut PANIC_DISPLAY: Option<Display> = None;


pub unsafe fn load_idt(idt_ptr: *const Idt) {
    let idtr = Idtr::new(idt_ptr);


    core::arch::asm!(
    "lidt [{}]",
    in(reg) &idtr,
    options(readonly, nostack, preserves_flags)
    );
}

pub static mut MY_IDT: Idt = Idt::new();

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
    let error_display = Display {
        ptr: ptr,
        back_ptr: back_ptr,
        width: resolution.0,
        height: resolution.1,
        stride: stride,
    };

    unsafe {
        PANIC_DISPLAY = Some(error_display);
    }
    
    unsafe {
        MY_IDT.init();
        display.rect(0, 0, 100, 100, 0xFF0000);

        unsafe {
            core::arch::asm!("cli"); // Вырубаем всё
            load_idt(&MY_IDT);
        }
        load_idt(&MY_IDT);
        display.rect(100, 0, 100, 100, 0x00FF00);
    }



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


        display.rect(200, 0, 100, 100, 0x0000FF);

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


const FONT: &[u8] = include_bytes!("IBM_VGA_8x16.bin");


#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
