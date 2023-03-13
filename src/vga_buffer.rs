use core::fmt::{self, Write};
use alloc::string::String;
use lazy_static::lazy_static;
use spin::Mutex;
use volatile::Volatile;

lazy_static! {
    /// A global `Writer` instance that can be used for printing to the VGA text buffer.
    ///
    /// Used by the `print!` and `println!` macros.
    pub static ref WRITER: Mutex<Writer> = Mutex::new({
        for c in 0..(BUFFER_WIDTH * BUFFER_HEIGHT) {
            unsafe {
                *((0xb8000 + c * 2) as *mut u16) = 0;
            }

        }
        Writer {
            column_position: 0,
            color_code: ColorCode::new(Color::Yellow, Color::Black),
            buffer: unsafe { &mut *(0xb8000 as *mut Buffer) },
            cmd_start: (0, 0) // should set in init();
        }
    });
}

/// The standard color palette in VGA text mode.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

pub static COLOR_LIST: [Color; 16] = 
    [Color::Black, Color::Blue, Color::Green, Color::Cyan, Color::Red, Color::Magenta, Color::Brown, Color::LightGray, Color::DarkGray, 
     Color::LightBlue, Color::LightGreen, Color::LightCyan, Color::LightRed, Color::Pink, Color::Yellow, Color::White];

pub static COLOR_NAME_LIST: [&'static str; 16] = 
    ["Black", "Blue", "Green", "Cyan", "Red", "Magenta", "Brown", "LightGray", "DarkGray",
     "LightBlue", "LightGreen", "LightCyan", "LightRed", "Pink", "Yellow", "White"];

/// A combination of a foreground and a background color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct ColorCode(u8);

impl ColorCode {
    /// Create a new `ColorCode` with the given foreground and background colors.
    pub fn new(foreground: Color, background: Color) -> ColorCode {
        ColorCode((background as u8) << 4 | (foreground as u8))
    }
}

/// A screen character in the VGA text buffer, consisting of an ASCII character and a `ColorCode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    ascii_character: u8,
    color_code: ColorCode,
}

/// The height of the text buffer (normally 25 lines).
pub const BUFFER_HEIGHT: usize = 25;
/// The width of the text buffer (normally 80 columns).
pub const BUFFER_WIDTH: usize = 80;

/// A structure representing the VGA text buffer.
#[repr(transparent)]
struct Buffer {
    chars: [[Volatile<ScreenChar>; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

/// A writer type that allows writing ASCII bytes and strings to an underlying `Buffer`.
///
/// Wraps lines at `BUFFER_WIDTH`. Supports newline characters and implements the
/// `core::fmt::Write` trait.
pub struct Writer {
    column_position: usize,
    color_code: ColorCode,
    buffer: &'static mut Buffer,
    // stuff for cmd, should extract
    cmd_start: (usize, usize) // row, col
}

impl Writer {
    /// Writes an ASCII byte to the buffer.
    ///
    /// Wraps lines at `BUFFER_WIDTH`. Supports the `\n` newline character.
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            byte => {
                if self.column_position >= BUFFER_WIDTH {
                    self.new_line();
                }

                let row = BUFFER_HEIGHT - 1;
                let col = self.column_position;

                let color_code = self.color_code;
                self.buffer.chars[row][col].write(ScreenChar {
                    ascii_character: byte,
                    color_code,
                });
                self.column_position += 1;
            }
        }
    }

    /// Writes the given ASCII string to the buffer.
    ///
    /// Wraps lines at `BUFFER_WIDTH`. Supports the `\n` newline character. Does **not**
    /// support strings with non-ASCII characters, since they can't be printed in the VGA text
    /// mode.
    fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                // printable ASCII byte or newline
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                // not part of printable ASCII range
                _ => self.write_byte(0xfe),
            }
        }
    }

    /// Shifts all lines one line up and clears the last row.
    fn new_line(&mut self) {
        for row in 1..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                let character = self.buffer.chars[row][col].read();
                self.buffer.chars[row - 1][col].write(character);
            }
        }
        self.clear_row(BUFFER_HEIGHT - 1);
        self.column_position = 0;
        if self.cmd_start.0 > 0 { self.cmd_start.0 -= 1; } // Decrease cmd start
        //else { panic!("Command goes off the screen, implement actual screenbuffer to fix!"); }
    }

    /// Clears a row by overwriting it with blank characters.
    pub fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar {
            ascii_character: b' ',
            color_code: self.color_code,
        };
        for col in 0..BUFFER_WIDTH {
            self.buffer.chars[row][col].write(blank);
        }
    }

    pub fn reset_screen(&mut self) {
        for row in 0..BUFFER_HEIGHT {
            self.clear_row(row);
        }
        self.column_position = 0;
    }

    pub fn backspace(&mut self) {
        // Assuming the last row
        let row = BUFFER_HEIGHT - 1;
        if self.column_position > 0 { 
            self.column_position -= 1;
        } 
        else {
            // Send everything down a row
            for row in (1..BUFFER_HEIGHT).rev() {
                for col in 0..BUFFER_WIDTH {
                    let character = self.buffer.chars[row - 1][col].read();
                    self.buffer.chars[row][col].write(character);
                }
            }
            self.clear_row(0);
            // Seek back to newline
            self.column_position = BUFFER_WIDTH - 1;
            while self.buffer.chars[BUFFER_HEIGHT - 1][self.column_position].read().ascii_character == 0 { self.column_position -= 1; }
        }
        self.buffer.chars[row][self.column_position].write(ScreenChar {
            ascii_character: 0,
            color_code: self.color_code,
        });
        
    }

    pub fn last_char(&self) -> char {
        self.buffer.chars[BUFFER_HEIGHT - 1][self.column_position - 1].read().ascii_character as char
    }
    pub fn scan_until_or_all(&self, c: char) -> String {
        let mut s = String::new();
        let mut row = BUFFER_HEIGHT - 1;
        let mut col = self.column_position - 1;
        while self.buffer.chars[row][col].read().ascii_character as char != c {//&& self.buffer.chars[row][col].read().ascii_character != 0 {
            if self.buffer.chars[row][col].read().ascii_character != 0 {
                s.insert(0, self.buffer.chars[row][col].read().ascii_character as char);
            }
            if col == 0 {
                col = BUFFER_WIDTH - 1;
                if row == 0 { break; }
                row -= 1;
            }
            else { col -= 1; }
        }
        s
    }
    pub fn reset_cmd_start(&mut self) {
        self.cmd_start = (BUFFER_HEIGHT - 1, self.column_position);
        //let start = self.cmd_start;
        //self.write_fmt(format_args!("{:?}", start)).unwrap();
    }
    pub fn scan_cmd(&self) -> String {
        let mut s = String::new();
        let mut row = BUFFER_HEIGHT - 1;
        let mut col = self.column_position;
        let start = self.cmd_start;
        while row > self.cmd_start.0 || ( row == self.cmd_start.0 && col >= self.cmd_start.1) {//&& self.buffer.chars[row][col].read().ascii_character != 0 {
            if self.buffer.chars[row][col].read().ascii_character != 0 {
                s.insert(0, self.buffer.chars[row][col].read().ascii_character as char);
            }
            if col == 0 {
                col = BUFFER_WIDTH - 1;
                if row == 0 { break; }
                row -= 1;
            }
            else { col -= 1; }
        }
        s
    }

    pub fn set_color(&mut self, color: ColorCode) {
        self.color_code = color;
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

/// Like the `print!` macro in the standard library, but prints to the VGA text buffer.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::vga_buffer::_print(format_args!($($arg)*)));
}


/// Like the `println!` macro in the standard library, but prints to the VGA text buffer.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}


/// Prints the given formatted string to the VGA text buffer
/// through the global `WRITER` instance.
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        WRITER.lock().write_fmt(args).unwrap();
    });
}

/* 
#[test_case]
fn test_println_simple() {
    println!("test_println_simple output");
}

#[test_case]
fn test_println_many() {
    for _ in 0..200 {
        println!("test_println_many output");
    }
}

#[test_case]
fn test_println_output() {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    let s = "Some test string that fits on a single line";
    interrupts::without_interrupts(|| {
        let mut writer = WRITER.lock();
        writeln!(writer, "\n{}", s).expect("writeln failed");
        for (i, c) in s.chars().enumerate() {
            let screen_char = writer.buffer.chars[BUFFER_HEIGHT - 2][i].read();
            assert_eq!(char::from(screen_char.ascii_character), c);
        }
    });
}
*/
