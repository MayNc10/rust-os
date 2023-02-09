use spin;
use x86_64;

pub static TIMER: spin::Mutex<u128> = 
    spin::Mutex::new(0);

pub fn read_timer() -> u128 {
    let mut time = 0;
    x86_64::instructions::interrupts::without_interrupts(||
        time = *TIMER.lock()
    );
    return time;
}