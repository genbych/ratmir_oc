use core::fmt;
use crate::display::{get_char_data, Display};

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
impl<'a> Console<'a> {

    pub fn scroll(&mut self) {

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

    pub fn write_char(&mut self, c: char) {

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

impl<'a> fmt::Write for Console<'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            self.write_char(c);
        }
        Ok(())
    }
}
