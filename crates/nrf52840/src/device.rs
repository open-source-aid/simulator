use pump_ypso::protocol::setting;
use pump_ypso::sim::{YpsoSim, PASSWORD_LEN};

pub type Device = YpsoSim;

const PASSWORD: [u8; PASSWORD_LEN] = [0x5A; PASSWORD_LEN];
const SERIAL_UPPER: i32 = 0xDEAD;
const SERIAL_LOWER: i32 = 0xBEEF;

#[cfg(not(feature = "opn"))]
fn base() -> Device {
    YpsoSim::new([0xA5; 32], PASSWORD)
}

#[cfg(feature = "opn")]
fn base() -> Device {
    YpsoSim::opn(PASSWORD)
}

pub fn new() -> Device {
    let mut device = base();
    device
        .settings
        .insert(setting::SERIAL_NUMBER_UPPER, SERIAL_UPPER);
    device
        .settings
        .insert(setting::SERIAL_NUMBER_LOWER, SERIAL_LOWER);
    device
}
