#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
extern crate alloc;

mod display;
mod console;
mod interrupts;
mod keyboard;
mod memory;
mod heap;

use core::panic::PanicInfo;
use uefi::prelude::*;
use uefi::{Char16};
use uefi::proto::console::gop::GraphicsOutput;
use uefi::table::boot::MemoryType;
use core::fmt::{write, Write};
use uefi::proto::console::text::{Key, ScanCode};
use display::{Display};
use console::{Console, TextGrid, BUFFER, STACK};
use interrupts::{kernel_panic, Idtr, Idt, IdtEntry, inb, outb};
use keyboard::{ scancode_to_char, KBD_BUFFER, KeyboardBuffer };
use crate::keyboard::{Spinlock, KBD_STATE};
use memory::{ BitmapAllocator, ALLOCATOR, map_page, PageTable, VirtAddr, Flags };
use crate::heap::FixedSizeBlockAllocator;

#[global_allocator]
static HEAP_ALLOCATOR: Spinlock<FixedSizeBlockAllocator> = Spinlock::new(FixedSizeBlockAllocator::new());

pub unsafe fn remap_pic() {
    // Порты Master PIC
    let master_command = 0x20;
    let master_data = 0x21;
    // Порты Slave PIC
    let slave_command = 0xA0;
    let slave_data = 0xA1;

    // Сохраняем текущие маски (необязательно, но полезно)
    let m1 = inb(master_data);
    let m2 = inb(slave_data);

    // 1. Начинаем инициализацию (ICW1)
    // 0x11 говорит PIC, что будет еще 3 байта настроек
    outb(master_command, 0x11);
    io_wait(); // Небольшая задержка для старого железа
    outb(slave_command, 0x11);
    io_wait();

    // 2. Устанавливаем смещение (ICW2)
    // Теперь Master IRQ 0-7 станут векторами 32-39 (0x20)
    outb(master_data, 0x20);
    io_wait();
    // Slave IRQ 8-15 станут векторами 40-47 (0x28)
    outb(slave_data, 0x28);
    io_wait();

    // 3. Соединяем контроллеры (ICW3)
    // Master говорит, что Slave подключен к IRQ 2
    outb(master_data, 0x04);
    io_wait();
    // Slave говорит свой идентификатор (2)
    outb(slave_data, 0x02);
    io_wait();

    // 4. Устанавливаем режим работы (ICW4)
    // 0x01 — это режим 8086 (самый обычный для нас)
    outb(master_data, 0x01);
    io_wait();
    outb(slave_data, 0x01);
    io_wait();

    // 5. Устанавливаем маски (OCW1)
    // 0x00 — разрешить ВСЕ прерывания.
    // Если хочешь только клаву и таймер, ставь 0xFC (11111100)
    outb(master_data, 0x00);
    outb(slave_data, 0x00);
}


fn io_wait() {
    unsafe {

        outb(0x80, 0);
    }
}

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

    ALLOCATOR.lock();
    let mut allocator = unsafe {&mut *ALLOCATOR.buf.get() };
    *allocator = Some(BitmapAllocator::new(mmap));

    ALLOCATOR.unlock();


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

        unsafe {
            core::arch::asm!("sti");
            load_idt(&MY_IDT);
        }
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


    ALLOCATOR.lock();
    let alloc = unsafe {&mut *ALLOCATOR.buf.get()};
    let pml4_addr = alloc.as_mut().unwrap().alloc();
    let pml4 = pml4_addr as *mut PageTable;
    unsafe { (*pml4).clear(); }

    ALLOCATOR.unlock();

    let phys_addr = 0x50000;
    let virt_addr_new = 0x1234_5678_9000;

    unsafe {
        map_page(&mut VirtAddr::new(0x50000), phys_addr, Flags::PRESENT | Flags::WRITABLE, pml4);
        map_page(&mut VirtAddr::new(virt_addr_new), phys_addr, Flags::PRESENT | Flags::WRITABLE, pml4);

        for addr in (0..0x1_0000_0000).step_by(0x1000) {
            map_page(&mut VirtAddr::new(addr), addr, Flags::PRESENT | Flags::WRITABLE, pml4);
        }
    }

    unsafe {
        core::arch::asm!(
        "mov cr3, {}",
        in(reg) pml4_addr,
        options(nostack, preserves_flags)
        );
    }


    display.rect(0, 0, display.width, display.height, 0x00000000);
    display.update();




    let heap_start = 0xFFFF_9000_0000_0000;
    let heap_size = 1024 * 1024;

    for addr in (heap_start..heap_start + heap_size).step_by(4096) {
        let mut v = VirtAddr::new(addr);

        ALLOCATOR.lock();

        let alloc = unsafe {&mut *ALLOCATOR.buf.get()};

        let phys = alloc.as_mut().unwrap().alloc();

        ALLOCATOR.unlock();

        unsafe {
            map_page(&mut v, phys, Flags::PRESENT | Flags::WRITABLE, pml4);
        }
    }

    unsafe {
        HEAP_ALLOCATOR.lock();

        let alloc = unsafe {&mut *HEAP_ALLOCATOR.buf.get()};

        alloc.fallback_allocator.init(heap_start as *mut u8, heap_size as usize);

        HEAP_ALLOCATOR.unlock();
    }


    use alloc::boxed::Box;
    use alloc::vec::Vec;

    let val = Box::new(1337);
    let mut v = Vec::new();
    v.push(*val);

    if v[0] == 1337 {
        write!(&mut console, "HEAP IS ALIVE! Box and Vec work. Next VAL: {}\n", v[0]);
    }
    else {
        write!(&mut console, "HEAP IS NOT ALIVE! Box and Vec work. Value: {} \n", v[0]);
    }

    let space_key = Char16::try_from('\r').unwrap();
    let backspace_key = Char16::try_from('\u{8}').unwrap();

    ALLOCATOR.lock();
    let mut allocator = unsafe {&mut *ALLOCATOR.buf.get() };

    write!(&mut console, "Allocated addr: 0x{:x} \n", allocator.as_mut().unwrap().alloc()).unwrap();
    write!(&mut console, "Allocated addr: 0x{:x} \n", allocator.as_mut().unwrap().alloc()).unwrap();
    write!(&mut console, "Allocated addr: 0x{:x} \n", allocator.as_mut().unwrap().alloc()).unwrap();

    ALLOCATOR.unlock();


    unsafe { let (st, _mmap) = st.exit_boot_services(MemoryType::LOADER_DATA); }


    unsafe {
        MY_IDT.init();
        load_idt(&MY_IDT);
        remap_pic();
        outb(0x21, 0xFC);
        core::arch::asm!("sti");
    }

    loop {

        let line_heights = console.line_lengths;

        KBD_BUFFER.lock();
        let buf = unsafe {&mut *KBD_BUFFER.buf.get()};
        let char = buf.get_char();
        KBD_BUFFER.unlock();
        


        if let Some(c) = char {
            if c == '\u{8}' {
                console.backspace();
            }
            else if c == '\n' {
                BUFFER.lock();
                let buf = unsafe {&mut *BUFFER.buf.get()};
                BUFFER.unlock();
                let (command, args) = console.command_handler(&buf.data);

                console.write_char('\n', false);


                if let Some(cmd) = command {
                    match cmd {
                        "echo" => { for arg in args { write!(&mut console, "{} ", arg).unwrap(); } console.write_char('\n', false) }
                        "help" => { write!(&mut console, "This is the help menu \n").unwrap(); }
                        _ => { write!(&mut console, "Unknown command {} \n", cmd).unwrap(); }
                    }
                }

                STACK.lock();
                BUFFER.lock();
                let mut buf = unsafe { &mut *BUFFER.buf.get() };
                let st = unsafe { &mut *STACK.buf.get() };

                st.push(*buf);
                buf.clear();

                STACK.unlock();
                BUFFER.unlock();

            }

            else {
                KBD_STATE.lock();
                let state = unsafe {&mut *KBD_STATE.buf.get()};

                if state.shift || state.caps {
                    console.write_char(c, true);
                }
                else {
                    console.write_char(c.to_ascii_lowercase(), true)
                }
                KBD_STATE.unlock();
            }
        }
        else {
            KBD_STATE.lock();
            let state = unsafe {&mut *KBD_STATE.buf.get()};

            if state.u_arr {
                STACK.lock();
                let old_buf = unsafe { &mut *STACK.buf.get() }.pop();
                STACK.unlock();

                if let Some(b) = old_buf {
                    BUFFER.lock();
                    unsafe { *BUFFER.buf.get() = b; }
                    BUFFER.unlock();

                    state.u_arr = false;

                    KBD_STATE.unlock();

                    console.display.clear_line(console.cursor_y);
                    let actual_len = b.cursor;
                    let s = core::str::from_utf8(&b.data[..actual_len]).unwrap_or("");

                    console.cursor_x = 0;
                    write!(&mut console, "{}", s).unwrap();

                    console.cursor_x = actual_len;

                    continue;
                }

            }

            else if state.d_arr {
                STACK.lock();
                let next_buf = unsafe { &mut *STACK.buf.get() }.next();
                STACK.unlock();

                if let Some(b) = next_buf {
                    BUFFER.lock();
                    unsafe { *BUFFER.buf.get() = b; }
                    BUFFER.unlock();

                    state.d_arr = false;

                    KBD_STATE.unlock();

                    console.display.clear_line(console.cursor_y);
                    let actual_len = b.cursor;
                    let s = core::str::from_utf8(&b.data[..actual_len]).unwrap_or("");

                    console.cursor_x = 0;
                    write!(&mut console, "{}", s).unwrap();

                    console.cursor_x = actual_len;


                    continue;
                }
            }

            KBD_STATE.unlock();
        }

        core::hint::spin_loop();
    }

    Status::SUCCESS

}




const FONT: &[u8] = include_bytes!("IBM_VGA_8x16.bin");


#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    kernel_panic("Panic");
    loop {}
}
