use core::alloc::{GlobalAlloc, Layout};
use core::ptr;
use core::ptr::{null_mut, NonNull};
use linked_list_allocator::Heap;
use crate::keyboard::Spinlock;

const BLOCK_SIZES: &[usize] = &[8, 16, 32, 64, 128, 256, 512, 1024, 2048];

struct ListNode {
    next: Option<&'static mut ListNode>,
}

pub struct  FixedSizeBlockAllocator {
    pub list_heads: [Option<&'static mut ListNode>; BLOCK_SIZES.len()],
    pub fallback_allocator: Heap
}


unsafe impl GlobalAlloc for Spinlock<FixedSizeBlockAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.lock();
        let alloc = unsafe { &mut *self.buf.get() };

        let result = match alloc.list_index(&layout) {
            Some(index) => {
                match alloc.list_heads[index].take() {
                    Some(node) => {
                        alloc.list_heads[index] = node.next.take();
                        node as *mut ListNode as *mut u8
                    }
                    None => {
                        alloc.fallback_alloc_and_fill(index)
                    }
                }
            }
            None => {
                alloc.fallback_allocator
                    .allocate_first_fit(layout)
                    .map_or(ptr::null_mut(), |ptr| ptr.as_ptr())
            }
        };

        self.unlock();
        result

    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.lock();
        let alloc = unsafe { &mut *self.buf.get() };

        match alloc.list_index(&layout) {
            Some(index) => {
                let new_node = ListNode { next: alloc.list_heads[index].take() };

                let node_ptr = ptr as *mut ListNode;
                unsafe {
                    node_ptr.write(new_node);
                    alloc.list_heads[index] = Some(&mut *node_ptr);
                }
            }
            None => {
                let ptr = NonNull::new(ptr).unwrap();
                alloc.fallback_allocator.deallocate(ptr, layout);
            }

        }

        self.unlock()
    }
}

impl FixedSizeBlockAllocator {
    pub const fn new() -> Self {
        const EMPTY: Option<&'static mut ListNode> = None;
        FixedSizeBlockAllocator {
            list_heads: [EMPTY; BLOCK_SIZES.len()],
            fallback_allocator: Heap::empty(),
        }
    }

    pub fn list_index(&self, layout: &Layout) -> Option<usize> {
        for i in 0..BLOCK_SIZES.len() {
            if BLOCK_SIZES[i] >= layout.size() && BLOCK_SIZES[i] >= layout.align() {
                return Some(i);
            }
        }

        None

    }

    pub fn slice(&mut self, start: u64, block_size: usize) {
        let page_end = start + 4096;
        let mut current_addr = start;

        loop {
            let next_addr = current_addr + block_size as u64;
            let node = unsafe { &mut *(current_addr as *mut ListNode) };

            if next_addr + block_size as u64 <= page_end {
                node.next = Some(unsafe { &mut *(next_addr as *mut ListNode) });
                current_addr = next_addr;
            } else {
                node.next = None;
                break;
            }
        }
    }
    pub fn fallback_alloc_and_fill(&mut self, index: usize) -> *mut u8 {
        let layout = Layout::from_size_align(4096, 4096).unwrap();

        match self.fallback_allocator.allocate_first_fit(layout) {
            Ok(ptr) => {
                let start = ptr.as_ptr() as u64;
                let block_size = BLOCK_SIZES[index];

                self.slice(start, block_size);

                let node = unsafe { &mut *(start as *mut ListNode) };
                self.list_heads[index] = node.next.take();

                start as *mut u8
            }
            Err(_) => ptr::null_mut(),
        }

    }
}