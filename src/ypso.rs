use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use pump_shared::{GattChar, NotifyKind, Uuid};
use pump_ypso::protocol::uuids;

use crate::Service;

pub const ADVERTISED: Uuid = uuids::PUMP_BASE_SERVICE;

const BASE: Uuid = uuids::PUMP_BASE_SERVICE;
const SETTINGS: Uuid = uuids::SETTINGS_SERVICE;
const HISTORY: Uuid = uuids::HISTORY_SERVICE;
const CONTROL: Uuid = uuids::CONTROL_SERVICE;
const CRYPTO: Uuid = uuids::CRYPTO_SERVICE;

fn plain(name: &'static str, service: Uuid, characteristic: Uuid) -> GattChar {
    GattChar {
        name,
        service,
        characteristic,
        notify: None,
    }
}

pub fn services(crypto: bool) -> Vec<Service> {
    let table = [
        plain("pump base version", BASE, uuids::PUMP_BASE_SERVICE_VERSION),
        plain("master software", BASE, uuids::MASTER_SOFTWARE_VERSION),
        plain(
            "supervisor software",
            BASE,
            uuids::SUPERVISOR_SOFTWARE_VERSION,
        ),
        plain("authorization", BASE, uuids::AUTHORIZATION_PASSWORD),
        plain("pairing passkey", BASE, uuids::PAIRING_PASSKEY),
        plain(
            "settings version",
            SETTINGS,
            uuids::SETTINGS_SERVICE_VERSION,
        ),
        plain("setting id", SETTINGS, uuids::SETTING_ID),
        plain("setting value", SETTINGS, uuids::SETTING_VALUE),
        plain("system date", SETTINGS, uuids::SYSTEM_DATE),
        plain("system time", SETTINGS, uuids::SYSTEM_TIME),
        plain("history version", HISTORY, uuids::HISTORY_SERVICE_VERSION),
        plain("alarm count", HISTORY, uuids::ALARM_ENTRY_COUNT),
        plain("alarm index", HISTORY, uuids::ALARM_ENTRY_INDEX),
        plain("alarm value", HISTORY, uuids::ALARM_ENTRY_VALUE),
        plain("event count", HISTORY, uuids::EVENT_ENTRY_COUNT),
        plain("event index", HISTORY, uuids::EVENT_ENTRY_INDEX),
        plain("event value", HISTORY, uuids::EVENT_ENTRY_VALUE),
        plain("system entry count", HISTORY, uuids::SYSTEM_ENTRY_COUNT),
        plain("system entry index", HISTORY, uuids::SYSTEM_ENTRY_INDEX),
        plain("system entry value", HISTORY, uuids::SYSTEM_ENTRY_VALUE),
        plain("complaint count", HISTORY, uuids::COMPLAINT_ENTRY_COUNT),
        plain("complaint index", HISTORY, uuids::COMPLAINT_ENTRY_INDEX),
        plain("complaint value", HISTORY, uuids::COMPLAINT_ENTRY_VALUE),
        plain("counter id", HISTORY, uuids::COUNTER_ID),
        plain("counter value", HISTORY, uuids::COUNTER_VALUE),
        plain("control version", CONTROL, uuids::CONTROL_SERVICE_VERSION),
        plain("start stop bolus", CONTROL, uuids::START_STOP_BOLUS),
        plain("bolus status", CONTROL, uuids::GET_BOLUS_STATUS),
        plain("start stop tbr", CONTROL, uuids::START_STOP_TBR),
        plain("system status", CONTROL, uuids::GET_SYSTEM_STATUS),
        GattChar {
            name: "bolus notification",
            service: CONTROL,
            characteristic: uuids::CONTROL_NOTIFY,
            notify: Some(NotifyKind::Notification),
        },
        plain("crypto version", CRYPTO, uuids::CRYPTO_SERVICE_VERSION),
        plain("new public key", CRYPTO, uuids::NEW_PUBLIC_KEY),
        plain("challenge", CRYPTO, uuids::WRITE_CHALLENGE_UTC_APP_PUB_KEY),
        plain("extended read", CRYPTO, uuids::EXTENDED_READ),
    ];
    crate::services(table.into_iter().filter(|c| c.service != CRYPTO || crypto))
}

pub fn name(serial: u32) -> String {
    format!("YpsoPump_{serial}")
}

#[cfg(feature = "ypso-sim")]
mod sim {
    use alloc::string::String;
    use alloc::vec::Vec;

    use pump_shared::Uuid;
    use pump_ypso::protocol::{assemble_serial, setting};
    use pump_ypso::sim::YpsoSim;

    use crate::{Command, Service, Simulator};

    impl Simulator for YpsoSim {
        fn name(&self) -> String {
            let part = |id| self.settings.get(&id).map_or(0, |&value| value as u16);
            super::name(assemble_serial(
                part(setting::SERIAL_NUMBER_UPPER),
                part(setting::SERIAL_NUMBER_LOWER),
            ))
        }

        fn advertised(&self) -> Uuid {
            super::ADVERTISED
        }

        fn services(&self) -> Vec<Service> {
            super::services(self.has_crypto_service())
        }

        fn apply(&mut self, command: Command) {
            match command {
                Command::Battery(level) => self.battery = level,
                Command::Reservoir(hundredths) => self.reservoir = hundredths,
                Command::Alarms(count) => self.alarm_count = count,
                Command::Reboot => self.reboot(),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use pump_shared::SimDevice;
        use pump_shared::{Address, BleOp, Timestamp};

        fn sim() -> YpsoSim {
            YpsoSim::new([0; 32], [0; 16])
        }

        #[test]
        fn advertises_its_serial_and_base_service() {
            let sim = sim();
            assert_eq!(sim.name(), "YpsoPump_10155555");
            assert_eq!(sim.advertised(), super::super::ADVERTISED);
            assert_eq!(sim.services().len(), 5);
        }

        #[test]
        fn applies_scenario_knobs() {
            let mut sim = sim();
            sim.apply(Command::Battery(1));
            sim.apply(Command::Reservoir(500));
            sim.apply(Command::Alarms(2));
            assert_eq!((sim.battery, sim.reservoir, sim.alarm_count), (1, 500, 2));
            let now = Timestamp::from_millis(0);
            let connect = BleOp::Connect {
                address: Address::new([0; 6]),
            };
            sim.handle(&connect, now);
            sim.apply(Command::Reboot);
            assert_eq!((sim.reboot, sim.is_connected()), (1, false));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_pump_after_its_serial() {
        assert_eq!(name(10_155_555), "YpsoPump_10155555");
    }

    #[test]
    fn hides_the_crypto_service_on_clear_text_pumps() {
        let encrypted = services(true);
        let clear = services(false);
        assert_eq!(encrypted.len(), 5);
        assert_eq!(clear.len(), 4);
        assert!(clear.iter().all(|s| s.uuid != CRYPTO));
        assert_eq!(
            encrypted
                .iter()
                .map(|s| s.characteristics.len())
                .sum::<usize>(),
            35
        );
    }
}
