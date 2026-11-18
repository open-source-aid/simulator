use alloc::string::String;
use alloc::vec::Vec;

use pump_shared::{SimDevice, Timestamp, Uuid};

use crate::{Command, Service};

pub trait Simulator: SimDevice {
    fn name(&self) -> String;

    fn advertised(&self) -> Uuid;

    fn services(&self) -> Vec<Service>;

    fn apply(&mut self, command: Command, now: Timestamp);
}
