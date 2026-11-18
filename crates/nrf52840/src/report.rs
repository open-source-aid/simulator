use core::cmp::min;
use core::fmt::{self, Display, Write};
use core::mem::MaybeUninit;
use core::ptr::{addr_of, addr_of_mut};
use core::str;

use cortex_m::peripheral::SCB;

const MAGIC: u32 = 0x5041_4E43;
const CAPACITY: usize = 480;
const BUILD: u64 = match u64::from_str_radix(env!("BUILD"), 10) {
    Ok(build) => build,
    Err(_) => panic!("BUILD is a number"),
};

#[repr(C)]
struct Report {
    magic: u32,
    len: usize,
    build: u64,
    text: [u8; CAPACITY],
}

#[link_section = ".uninit.report"]
static mut REPORT: MaybeUninit<Report> = MaybeUninit::uninit();

impl Write for Report {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let mut n = min(s.len(), CAPACITY - self.len);
        while !s.is_char_boundary(n) {
            n -= 1;
        }
        self.text[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

pub fn save(message: impl Display) -> ! {
    let report = unsafe { &mut *addr_of_mut!(REPORT).cast::<Report>() };
    report.len = 0;
    let _ = write!(report, "{message}");
    report.build = BUILD;
    report.magic = MAGIC;
    SCB::sys_reset()
}

pub fn stored() -> Option<&'static str> {
    let report = unsafe { &*addr_of!(REPORT).cast::<Report>() };
    let valid = report.magic == MAGIC && report.build == BUILD && report.len <= CAPACITY;
    valid.then(|| str::from_utf8(&report.text[..report.len]).unwrap_or("unreadable panic report"))
}

pub fn clear() {
    unsafe { (*addr_of_mut!(REPORT).cast::<Report>()).magic = 0 };
}
