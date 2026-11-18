use pump_ypso::sim::{YpsoSim, PASSWORD_LEN};

pub type Device = YpsoSim;

const PASSWORD: [u8; PASSWORD_LEN] = [0x5A; PASSWORD_LEN];

#[cfg(not(feature = "opn"))]
pub fn new() -> Device {
    YpsoSim::new([0xA5; 32], PASSWORD)
}

#[cfg(feature = "opn")]
pub fn new() -> Device {
    YpsoSim::opn(PASSWORD)
}
