#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;
#[cfg(test)]
extern crate std;

mod bridge;
mod console;
mod gatt;
mod simulator;
#[cfg(feature = "ypso")]
pub mod ypso;

pub use bridge::{AttError, Bridge, Outgoing};
pub use console::{Alarm, Command, Lines, ParseError};
pub use gatt::{services, Attribute, Attributes, Service};
pub use simulator::Simulator;
