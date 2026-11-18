use core::ptr;

use embassy_time::Timer;

pub const RED: u32 = 1 << 26;
pub const GREEN: u32 = 1 << 30;
pub const BLUE: u32 = 1 << 6;
const ALL: u32 = RED | GREEN | BLUE;
const OUTSET: *mut u32 = 0x5000_0508 as _;
const OUTCLR: *mut u32 = 0x5000_050C as _;
const DIRSET: *mut u32 = 0x5000_0518 as _;

pub fn show(color: u32) {
    unsafe {
        ptr::write_volatile(OUTSET, ALL);
        ptr::write_volatile(DIRSET, ALL);
        ptr::write_volatile(OUTCLR, color);
    }
}

pub async fn blink(color: u32) -> ! {
    loop {
        show(color);
        Timer::after_millis(500).await;
        show(0);
        Timer::after_millis(500).await;
    }
}
