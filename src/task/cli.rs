use core::str::SplitAsciiWhitespace;
use spin::Mutex;

use crate::{print, println, vga_buffer::{WRITER, Color, COLOR_LIST, ColorCode, BUFFER_HEIGHT, COLOR_NAME_LIST, BUFFER_WIDTH}, disk::pio::DRIVER};
use conquer_once::spin::OnceCell;
use lazy_static::lazy_static;
use alloc::string::String;
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
use futures_util::{
    stream::{Stream, StreamExt},
    task::AtomicWaker,
};

use super::keyboard::{ScancodeStream, DISK_WRITER, text_edit_process_key};

pub static ESC: char = 0x1B as char;
pub static BUFFER_CHAR: char = 0x2 as char;

// just a hack to enable text editor, is not extensible at all
lazy_static! {
    pub static ref IS_TEXT_MODE: Mutex<bool> = Mutex::new(false);
}


pub async fn cli() {
    let mut scancodes = ScancodeStream::new();
    let mut keyboard = Keyboard::new(layouts::Us104Key, ScancodeSet1, HandleControl::Ignore);

    while let Some(scancode) = scancodes.next().await {
        if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
            if let Some(key) = keyboard.process_keyevent(key_event) {
                // just a garbage hack 
                if *IS_TEXT_MODE.lock() {
                    if let DecodedKey::Unicode(c) = key && c == ESC {
                        // leave text edit mode
                        *IS_TEXT_MODE.lock() = false;
                        WRITER.lock().reset_screen();
                        print!("$> ");
                        WRITER.lock().reset_cmd_start();
                    }
                    else {
                        text_edit_process_key(key);
                    }
                } else {
                    match key {
                        DecodedKey::Unicode(character) => {
                            if character as u32 == 8 {
                                let mut writer = WRITER.lock();
                                let cur_pos = writer.current_pos();
                                let start = writer.cmd_start();
                                let pos = |pos: (usize, usize)| pos.0 * BUFFER_WIDTH + pos.1;
                                if pos(cur_pos) > pos(start) { writer.backspace(); }
                            }
                            else if character == '\n' as char {
                                println!();
                                let command = WRITER.lock().scan_cmd();
                                handle_command(command);
                                //println!("{}", command);
                                if !*IS_TEXT_MODE.lock() {
                                    print!("$> ");
                                    WRITER.lock().reset_cmd_start();
                                }
                            } 
                            else {
                                print!("{}", character);
                            }
                        },
                        DecodedKey::RawKey(key) => print!("{:?} ", key),
                    }
                }
            }
        }
    } 
}

fn handle_command(command: String) {
    let mut parts = command.split_ascii_whitespace();
    let command = parts.next();
    if command.is_none() { 
        // empty command, return
        println!("Error: empty command");
        return;
    }
    let command = command.unwrap();
    match command {
        "cat" => cat(parts),
        "color" => color(parts),
        "dclear" => dclear(parts),
        "dappend" => dappend(parts),
        "textedit" => {
            WRITER.lock().reset_screen();
            // just hack
            *IS_TEXT_MODE.lock() = true;
            // dump disk contents
            let writer =  DISK_WRITER.lock();
            for b in &writer.current_buf[0..writer.current_buf_offset as usize] {
                print!("{}{}", (b & 0xFF) as u8 as char, (b >> 8) as u8 as char);
            }
            if writer.is_in_word {
                print!("{}", (writer.current_buf[writer.current_buf_offset as usize] & 0xff) as u8 as char); 
            }
        }, 
        "echo" => echo(parts),
        "help" => help(parts),
        _ => println!("Error: unrecognized command {}", command),
    }
}

fn echo(args: SplitAsciiWhitespace) {
    println!("{} ", args.into_iter().intersperse(&" ").collect::<String>());
}

fn help(_args: SplitAsciiWhitespace) {
    println!("List of commands:");
    println!("  cat: prints the contents of the disk to screen");
    println!("  color [fg] [bg]: sets the foreground of the terminal to fg and the background to bg");
    println!("      [fg] and [bg] can either be numbers or the names of colors");
    println!("      currently, the supported colors are:");
    for color in COLOR_NAME_LIST {
        println!("      {}", color);
    }
    println!("  dclear - clear the contents of the disk");
    println!("  dappend [...]: appends any text that follows to the disk");
    println!("  textedit: opens a text editor that writes to the screen and to the disk");
    println!("      to get back to the terminal, press ESC");
    println!("  echo [...]: prints any text that follows to the screen");
    println!("  help: prints this help message");
}

fn color(mut args: SplitAsciiWhitespace) {
    let fg = args.next();
    if fg.is_none() {
        println!("Error: missing foreground color");
        return;
    }
    let fg = fg.unwrap();

    let bg = args.next();
    if bg.is_none() {
        println!("Error: missing foreground color");
        return;
    }
    if args.next().is_some() {
        println!("Error: only 2 arguments expected");
        return;
    }
    let bg = bg.unwrap();

    let fg = {
        if let Ok(color) = fg.parse::<usize>() {
            // is a number
            if color > COLOR_LIST.len() {
                println!("Error: invalid color");
                // explain what the valid colors are
                return;
            }
            COLOR_LIST[color]
        }
        else {
            COLOR_LIST[
                match fg {
                    "Black" => 0,
                    "Blue" => 1,
                    "Green" => 2,
                    "Cyan" => 3,
                    "Red" => 4,
                    "Magenta" => 5,
                    "Brown" => 6,
                    "LightGray" => 7,
                    "DarkGray" => 8,
                    "LightBlue" => 9,
                    "LightGreen" => 10,
                    "LightCyan" => 11,
                    "LightRed" => 12,
                    "Pink" => 13,
                    "Yellow" => 14,
                    "White" => 15,
                    _ => {
                        println!("Error: invalid color");
                        // explain what the valid colors are
                        return;
                    }
                } 
            ]
        }
    };

    let bg = {
        if let Ok(color) = bg.parse::<usize>() {
            // is a number
            if color > COLOR_LIST.len() {
                println!("Error: invalid color");
                // explain what the valid colors are
                return;
            }
            COLOR_LIST[color]
        }
        else {
            COLOR_LIST[
                match bg {
                    "Black" => 0,
                    "Blue" => 1,
                    "Green" => 2,
                    "Cyan" => 3,
                    "Red" => 4,
                    "Magenta" => 5,
                    "Brown" => 6,
                    "LightGray" => 7,
                    "DarkGray" => 8,
                    "LightBlue" => 9,
                    "LightGreen" => 10,
                    "LightCyan" => 11,
                    "LightRed" => 12,
                    "Pink" => 13,
                    "Yellow" => 14,
                    "White" => 15,
                    _ => {
                        println!("Error: invalid color");
                        // explain what the valid colors are
                        return;
                    }
                } 
            ]
        }
    };

    let new_color = ColorCode::new(fg, bg);
    WRITER.lock().set_color(new_color);
}

pub fn dclear(mut args: SplitAsciiWhitespace) {
    if args.next().is_some() {
        println!("Error: 0 arguments expected");
        return
    }

    let mut writer = DISK_WRITER.lock();
    // erase data
    let mut blank = [0; 256];
    for lba in 0..(writer.current_lba + 1) { // lbas are also zero-indexed, so we add one to get the last one
        DRIVER.lock().write(&mut blank, lba, 1);
    }
    writer.current_buf = blank;
    writer.current_buf_offset = 0;
    writer.current_lba = 0;
}

fn cat(mut args: SplitAsciiWhitespace) {
    if args.next().is_some() {
        println!("Error: 0 arguments expected");
        return;
    }
    let writer = DISK_WRITER.lock();
    // read full sectors
    let mut buf = [0; 256];
    for lba in 0..writer.current_lba { // lbas zero-indexed
        DRIVER.lock().read(&mut buf, lba, 1);
        for b in buf {
            print!("{}{}", (b & 0xFF) as u8 as char, (b >> 8)as u8 as char);
        }
    }
    for b in &writer.current_buf[0..writer.current_buf_offset as usize] {
        print!("{}{}", (b & 0xFF) as u8 as char, (b >> 8)as u8 as char);
    }
    println!();
}

fn dappend(args: SplitAsciiWhitespace) {

    for c in args.into_iter().intersperse(&" ").flat_map(|s| s.chars())  {
        //print!("{c}");
        let mut writer = DISK_WRITER.lock();
        let off = writer.current_buf_offset as usize;
        if !writer.is_in_word {
            writer.current_buf[off] |= c as u32 as u16;
        } else {
            writer.current_buf[off] |= (c as u32 as u16) << 8; // set high bytes
        }
        if writer.is_in_word { writer.current_buf_offset += 1; }
        writer.is_in_word = !writer.is_in_word;
    }
    //println!("\nFlushing Buffer!");
    // Flush buffer 
    let mut writer = DISK_WRITER.lock();
    let lba = writer.current_lba;
    x86_64::instructions::interrupts::without_interrupts(||
        DRIVER.lock().write(&mut writer.current_buf, lba, 1));

    
    if writer.current_buf_offset == 256 {
        // go to next sector
        // first, output the current cached buf
        
        writer.current_lba += 1;
        writer.current_buf_offset = 0;
        writer.is_in_word = false;
        let lba = writer.current_lba;
        x86_64::instructions::interrupts::without_interrupts(||
            DRIVER.lock().read(&mut writer.current_buf, lba, 1));
    }
    //println!("Finished flushing buffer!");
}