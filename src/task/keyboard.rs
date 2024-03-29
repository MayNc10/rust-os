use crate::{print, println, vga_buffer::{WRITER, BUFFER_WIDTH}, disk::pio};
use conquer_once::spin::OnceCell;
use lazy_static::lazy_static;
use spin::Mutex;
use core::{
    pin::Pin,
    task::{Context, Poll},
};
use crossbeam_queue::ArrayQueue;
use futures_util::{
    stream::{Stream, StreamExt},
    task::AtomicWaker,
};
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};

static SCANCODE_QUEUE: OnceCell<ArrayQueue<u8>> = OnceCell::uninit();
static WAKER: AtomicWaker = AtomicWaker::new();

/// Called by the keyboard interrupt handler
///
/// Must not block or allocate.
pub(crate) fn add_scancode(scancode: u8) {
    if let Ok(queue) = SCANCODE_QUEUE.try_get() {
        if let Err(_) = queue.push(scancode) {
            println!("WARNING: scancode queue full; dropping keyboard input");
        } else {
            WAKER.wake();
        }
    } else {
        println!("WARNING: scancode queue uninitialized");
    }
}

pub struct ScancodeStream {
    _private: (),
}

impl ScancodeStream {
    pub fn new() -> Self {
        SCANCODE_QUEUE
            .try_init_once(|| ArrayQueue::new(100))
            .expect("ScancodeStream::new should only be called once");
        ScancodeStream { _private: () }
    }
}

impl Stream for ScancodeStream {
    type Item = u8;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<u8>> {
        let queue = SCANCODE_QUEUE
            .try_get()
            .expect("scancode queue not initialized");

        // fast path
        if let Ok(scancode) = queue.pop() {
            return Poll::Ready(Some(scancode));
        }

        WAKER.register(&cx.waker());
        match queue.pop() {
            Ok(scancode) => {
                WAKER.take();
                Poll::Ready(Some(scancode))
            }
            Err(crossbeam_queue::PopError) => Poll::Pending,
        }
    }
}

pub async fn print_keypresses() {
    let mut scancodes = ScancodeStream::new();
    let mut keyboard = Keyboard::new(layouts::Us104Key, ScancodeSet1, HandleControl::Ignore);

    while let Some(scancode) = scancodes.next().await {
        if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
            if let Some(key) = keyboard.process_keyevent(key_event) {
                match key {
                    DecodedKey::Unicode(character) => {
                        if character as u32 == 8 {
                            let mut writer = WRITER.lock();
                            let cur_pos = writer.current_pos();
                            let start = writer.cmd_start();
                            let pos = |pos: (usize, usize)| pos.0 * BUFFER_WIDTH + pos.1;
                            if pos(cur_pos) > pos(start) { writer.backspace(); }
                        } else {
                            print!("{}", character);
                        }
                    },
                    DecodedKey::RawKey(key) => print!("{:?} ", key),
                }
            }
        }
    }
}

pub struct DiskWriter {
    pub current_lba: u32,
    pub current_buf: [u16; 256],
    pub current_buf_offset: u16,
    pub is_in_word: bool,
}
impl DiskWriter {
    pub unsafe fn init(&mut self) {
        // kinda hacky, assume we never write a 0 into the disk ourselves
        let mut lba = 0;
        let mut buf = [0; 256];
        while {
            pio::DRIVER.lock().read(&mut buf, lba, 1);
            let last_written_pos = buf.iter().position(|v| *v == 0);
            if let Some(p) = last_written_pos {
                self.current_buf_offset = p as u16;
                if self.current_buf_offset != 0 && (buf[self.current_buf_offset as usize - 1] >> 8) == 0 {
                    self.current_buf_offset -= 1;
                    self.is_in_word = true;
                }
                false
            } else { true }

        } {
            lba += 1;
        }
        self.current_lba = lba;
        self.current_buf = buf;
    }
}

lazy_static! {
    pub static ref DISK_WRITER: Mutex<DiskWriter> = Mutex::new(DiskWriter { 
        current_lba: 0, 
        current_buf: [0; 256], 
        current_buf_offset: 0, 
        is_in_word: false,
    });
}

pub async fn text_editor() {
    let mut scancodes = ScancodeStream::new();
    let mut keyboard = Keyboard::new(layouts::Us104Key, ScancodeSet1, HandleControl::Ignore);

    while let Some(scancode) = scancodes.next().await {
        if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
            if let Some(key) = keyboard.process_keyevent(key_event) {
                text_edit_process_key(key);
            }
        }
    }
}

pub fn text_edit_process_key(key: DecodedKey) {
    match key {
        DecodedKey::Unicode(character) => {
            if character as u32 == 8 {
                // this isn't great, but it kinda works so we'll roll with it
                WRITER.lock().backspace();
                let mut writer = DISK_WRITER.lock();
                // first, try to move back
                if writer.current_buf_offset == 0 && !writer.is_in_word {
                    if writer.current_lba != 0 {
                        writer.current_lba -= 1;
                        writer.current_buf_offset = 255;
                        let lba = writer.current_lba;
                        x86_64::instructions::interrupts::without_interrupts(||
                            pio::DRIVER.lock().read(&mut writer.current_buf, lba, 1));
                    }
                }
                else if !writer.is_in_word { writer.current_buf_offset -= 1; }
                writer.is_in_word = !writer.is_in_word;
                
                let off = writer.current_buf_offset as usize;
                if !writer.is_in_word {
                    writer.current_buf[off] = 0;
                } else {
                    writer.current_buf[off] &= 0xFF; // clear high bytes
                }
                
                // Flush buffer
                let lba = writer.current_lba;
                x86_64::instructions::interrupts::without_interrupts(||
                    pio::DRIVER.lock().write(&mut writer.current_buf, lba, 1));
            } else {
                print!("{}", character);
                let mut writer = DISK_WRITER.lock();
                let off = writer.current_buf_offset as usize;
                if !writer.is_in_word {
                    writer.current_buf[off] |= character as u32 as u16;
                } else {
                    writer.current_buf[off] |= (character as u32 as u16) << 8; // set high bytes
                }
                if writer.is_in_word { writer.current_buf_offset += 1; }
                writer.is_in_word = !writer.is_in_word;

                // Flush buffer (not much of a buffer I know)
                let lba = writer.current_lba;
                x86_64::instructions::interrupts::without_interrupts(||
                    pio::DRIVER.lock().write(&mut writer.current_buf, lba, 1));

                
                if writer.current_buf_offset == 256 {
                    // go to next sector
                    // first, output the current cached buf
                    
                    writer.current_lba += 1;
                    writer.current_buf_offset = 0;
                    writer.is_in_word = false;
                    let lba = writer.current_lba;
                    x86_64::instructions::interrupts::without_interrupts(||
                        pio::DRIVER.lock().read(&mut writer.current_buf, lba, 1));
                }
                //println!("Leaving buffer step");
            }
        },
        DecodedKey::RawKey(_key) => {},
    }
}
