use crate::FONT;

pub struct Display {
    pub ptr: *mut u32,
    pub back_ptr: *mut u32,
    pub width: usize,
    pub height: usize,
    pub stride: usize,
}

impl Display {
    pub fn write_pixel(&self, x: usize, y: usize, color: u32) {
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

    pub fn update(&self) {
        unsafe {
            core::ptr::copy_nonoverlapping(self.back_ptr, self.ptr, self.stride * self.height)
        }
    }

    pub fn update_region(&self, x: usize, y: usize, w: usize, h: usize) {
        unsafe {
            for row in y..(y+h) {
                let offset = self.stride * row + x;
                let src = self.back_ptr.add(offset);
                let dst = self.ptr.add(offset);

                core::ptr::copy_nonoverlapping(src, dst, w);
            }
        }
    }
    
    pub fn rect(&self, x: usize, y: usize, mut w: usize, mut h: usize, color: u32) {
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

    pub fn clear_line(&self, line_index: usize) {
        unsafe {
            let offset = line_index * 16 * self.stride;
            let line_ptr = self.back_ptr.add(offset);

            let count = self.stride * 16;

            core::ptr::write_bytes(line_ptr, 0, count);
        }
    }

    pub fn write_char(&self, c: char, x: usize, y: usize, color: u32) {
        let char_data = get_char_data(c);
        for row in 0..16 {
            let row_data = char_data[row];
            for bit in 0..8 {
                if (row_data & (0x80 >> bit)) != 0 {
                    self.write_pixel(x + bit, y + row, color);
                }
            }
        }
    }


    pub fn write_str_at(&self, s: &str, x: usize, mut y: usize, color: u32) {
        let mut current_x = x;
        for c in s.chars() {
            if c == '\n' {
                y += 16;
                current_x = x;
            } else {
                self.write_char(c, current_x, y, color);
                current_x += 8;
            }
        }
    }
}

pub fn get_char_data<'a>(c: char) -> &'a[u8] {
    let code = c as usize;

    let start = code * 16;
    let end = start + 16;

    &FONT[start..end]
}