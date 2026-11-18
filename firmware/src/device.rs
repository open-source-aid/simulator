#[cfg(feature = "ypso")]
mod ypso {
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
}

#[cfg(not(feature = "ypso"))]
mod ypso {
    use alloc::string::String;
    use alloc::vec::Vec;

    use pump_shared::{BleOp, BleResult, SimDevice, Timestamp, Uuid};
    use pump_ypso::protocol::uuids;
    use simulator_nrf52840::{ypso, Command, Service, Simulator};

    pub struct Device;

    const VERSIONS: [(Uuid, &str); 5] = [
        (uuids::PUMP_BASE_SERVICE_VERSION, "1.2"),
        (uuids::MASTER_SOFTWARE_VERSION, "V05.02.03"),
        (uuids::SETTINGS_SERVICE_VERSION, "1.7"),
        (uuids::HISTORY_SERVICE_VERSION, "1.4"),
        (uuids::CONTROL_SERVICE_VERSION, "1.4"),
    ];

    pub fn new() -> Device {
        Device
    }

    impl SimDevice for Device {
        fn handle(&mut self, op: &BleOp, _now: Timestamp) -> BleResult {
            match op {
                BleOp::Read { characteristic, .. } => VERSIONS
                    .iter()
                    .find(|(uuid, _)| uuid == characteristic)
                    .map_or(BleResult::Failed, |(_, version)| {
                        BleResult::Data(version.as_bytes().to_vec())
                    }),
                _ => BleResult::Done,
            }
        }
    }

    impl Simulator for Device {
        fn name(&self) -> String {
            ypso::name(10_155_555)
        }

        fn advertised(&self) -> Uuid {
            ypso::ADVERTISED
        }

        fn services(&self) -> Vec<Service> {
            ypso::services(true)
        }

        fn apply(&mut self, _command: Command) {}
    }
}

pub use ypso::{new, Device};
