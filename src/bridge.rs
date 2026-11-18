use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::fmt;

use pump_shared::{Address, BleOp, BleResult, GattChar, NotifyKind, SimDevice, Timestamp, Uuid};

use crate::{Attribute, Attributes};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AttError(pub u8);

impl AttError {
    pub const INVALID_HANDLE: Self = Self(0x01);
    pub const INVALID_OFFSET: Self = Self(0x07);
    pub const INVALID_LENGTH: Self = Self(0x0D);
    pub const UNLIKELY: Self = Self(0x0E);
    pub const IMPROPER_CCCD: Self = Self(0xFD);
}

impl fmt::Display for AttError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ATT error 0x{:02X}", self.0)
    }
}

impl core::error::Error for AttError {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Outgoing {
    pub handle: u16,
    pub kind: NotifyKind,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct Bridge<D> {
    device: D,
    attributes: Attributes,
    subscriptions: BTreeMap<(Uuid, Uuid), (u16, NotifyKind)>,
    last_read: Option<(u16, Vec<u8>)>,
}

impl<D: SimDevice> Bridge<D> {
    pub fn new(device: D, attributes: Attributes) -> Self {
        Self {
            device,
            attributes,
            subscriptions: BTreeMap::new(),
            last_read: None,
        }
    }

    pub fn device(&mut self) -> &mut D {
        &mut self.device
    }

    pub fn connect(&mut self, address: Address, now: Timestamp) -> Result<(), AttError> {
        self.forget();
        done(self.device.handle(&BleOp::Connect { address }, now))
    }

    pub fn disconnect(&mut self, now: Timestamp) -> Result<(), AttError> {
        self.forget();
        done(self.device.handle(&BleOp::Disconnect, now))
    }

    pub fn read(
        &mut self,
        handle: u16,
        offset: usize,
        now: Timestamp,
    ) -> Result<Vec<u8>, AttError> {
        if offset == 0 {
            let Some(Attribute::Value(characteristic)) = self.attributes.get(handle) else {
                return Err(AttError::INVALID_HANDLE);
            };
            let op = BleOp::Read {
                service: characteristic.service,
                characteristic: characteristic.characteristic,
            };
            self.last_read = Some((handle, data(self.device.handle(&op, now))?));
        }
        match &self.last_read {
            Some((last, value)) if *last == handle && offset <= value.len() => Ok(value.clone()),
            _ => Err(AttError::INVALID_OFFSET),
        }
    }

    pub fn write(&mut self, handle: u16, data: &[u8], now: Timestamp) -> Result<(), AttError> {
        match self.attributes.get(handle) {
            Some(Attribute::Value(characteristic)) => {
                let op = BleOp::Write {
                    service: characteristic.service,
                    characteristic: characteristic.characteristic,
                    data: data.to_vec(),
                };
                done(self.device.handle(&op, now))
            }
            Some(Attribute::Cccd {
                characteristic,
                value,
            }) => self.configure(characteristic, value, data, now),
            None => Err(AttError::INVALID_HANDLE),
        }
    }

    pub fn poll(&mut self, now: Timestamp) -> Option<Outgoing> {
        loop {
            let notification = self.device.poll_notification(now)?;
            let key = (notification.service, notification.characteristic);
            if let Some(&(handle, kind)) = self.subscriptions.get(&key) {
                return Some(Outgoing {
                    handle,
                    kind,
                    data: notification.data,
                });
            }
        }
    }

    fn configure(
        &mut self,
        characteristic: GattChar,
        value: u16,
        data: &[u8],
        now: Timestamp,
    ) -> Result<(), AttError> {
        let &[bits, _] = data else {
            return Err(AttError::INVALID_LENGTH);
        };
        let service = characteristic.service;
        let uuid = characteristic.characteristic;
        self.subscriptions.remove(&(service, uuid));
        let kind = match bits & 0b11 {
            0 => {
                let op = BleOp::Unsubscribe {
                    service,
                    characteristic: uuid,
                };
                return done(self.device.handle(&op, now));
            }
            0b01 => NotifyKind::Notification,
            _ => NotifyKind::Indication,
        };
        if characteristic.notify != Some(kind) {
            return Err(AttError::IMPROPER_CCCD);
        }
        let op = BleOp::Subscribe {
            service,
            characteristic: uuid,
            kind,
        };
        done(self.device.handle(&op, now))?;
        self.subscriptions.insert((service, uuid), (value, kind));
        Ok(())
    }

    fn forget(&mut self) {
        self.subscriptions.clear();
        self.last_read = None;
    }
}

fn data(result: BleResult) -> Result<Vec<u8>, AttError> {
    match result {
        BleResult::Data(value) => Ok(value),
        other => Err(failure(other)),
    }
}

fn done(result: BleResult) -> Result<(), AttError> {
    match result {
        BleResult::Done => Ok(()),
        other => Err(failure(other)),
    }
}

fn failure(result: BleResult) -> AttError {
    match result {
        BleResult::Status(code) if code != 0 => AttError(code),
        _ => AttError::UNLIKELY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::VecDeque;
    use alloc::string::ToString;
    use alloc::vec;
    use pump_shared::Notification;

    const SERVICE: Uuid = Uuid::from_bytes([1; 16]);
    const ADDRESS: Address = Address::new([1, 2, 3, 4, 5, 6]);

    fn chr(byte: u8, notify: Option<NotifyKind>) -> GattChar {
        GattChar {
            name: "c",
            service: SERVICE,
            characteristic: Uuid::from_bytes([byte; 16]),
            notify,
        }
    }

    fn version() -> GattChar {
        chr(2, None)
    }

    fn status() -> GattChar {
        chr(3, Some(NotifyKind::Notification))
    }

    fn progress() -> GattChar {
        chr(4, Some(NotifyKind::Indication))
    }

    #[derive(Default)]
    struct Fake {
        ops: Vec<BleOp>,
        answers: VecDeque<BleResult>,
        notifications: VecDeque<Notification>,
    }

    impl SimDevice for Fake {
        fn handle(&mut self, op: &BleOp, now: Timestamp) -> BleResult {
            assert_eq!(now, Timestamp::from_millis(7));
            self.ops.push(op.clone());
            self.answers.pop_front().unwrap_or(BleResult::Done)
        }

        fn poll_notification(&mut self, _now: Timestamp) -> Option<Notification> {
            self.notifications.pop_front()
        }
    }

    const NOW: Timestamp = Timestamp::from_millis(7);

    fn bridge(answers: impl IntoIterator<Item = BleResult>) -> Bridge<Fake> {
        let mut attributes = Attributes::default();
        attributes.bind(version(), 10, None);
        attributes.bind(status(), 20, Some(21));
        attributes.bind(progress(), 30, Some(31));
        let device = Fake {
            answers: answers.into_iter().collect(),
            ..Fake::default()
        };
        Bridge::new(device, attributes)
    }

    fn notification(characteristic: GattChar, byte: u8) -> Notification {
        Notification {
            service: SERVICE,
            characteristic: characteristic.characteristic,
            data: vec![byte],
        }
    }

    #[test]
    fn maps_connection_events_to_link_operations() {
        let mut bridge = bridge([BleResult::Done, BleResult::Disconnected]);
        assert_eq!(bridge.connect(ADDRESS, NOW), Ok(()));
        assert_eq!(bridge.disconnect(NOW), Err(AttError::UNLIKELY));
        assert_eq!(
            bridge.device().ops,
            [BleOp::Connect { address: ADDRESS }, BleOp::Disconnect]
        );
    }

    #[test]
    fn answers_reads_with_the_device_value() {
        let mut bridge = bridge([BleResult::Data(vec![1, 2, 3])]);
        assert_eq!(bridge.read(10, 0, NOW), Ok(vec![1, 2, 3]));
        assert_eq!(
            bridge.device().ops,
            [BleOp::Read {
                service: SERVICE,
                characteristic: version().characteristic
            }]
        );
    }

    #[test]
    fn continues_a_long_read_from_the_first_answer() {
        let mut bridge = bridge([BleResult::Data(vec![1, 2, 3])]);
        bridge.read(10, 0, NOW).unwrap();
        assert_eq!(bridge.read(10, 2, NOW), Ok(vec![1, 2, 3]));
        assert_eq!(bridge.read(10, 4, NOW), Err(AttError::INVALID_OFFSET));
        assert_eq!(bridge.read(20, 1, NOW), Err(AttError::INVALID_OFFSET));
        assert_eq!(bridge.device().ops.len(), 1);
    }

    #[test]
    fn a_long_read_needs_a_first_read() {
        let mut bridge = bridge([]);
        assert_eq!(bridge.read(10, 1, NOW), Err(AttError::INVALID_OFFSET));
    }

    #[test]
    fn maps_device_failures_to_att_errors() {
        let mut bridge = bridge([
            BleResult::Status(0x83),
            BleResult::Status(0),
            BleResult::Timeout,
            BleResult::Done,
            BleResult::Status(0x05),
            BleResult::Data(vec![1]),
        ]);
        assert_eq!(bridge.read(10, 0, NOW), Err(AttError(0x83)));
        assert_eq!(bridge.read(10, 0, NOW), Err(AttError::UNLIKELY));
        assert_eq!(bridge.read(10, 0, NOW), Err(AttError::UNLIKELY));
        assert_eq!(bridge.read(10, 0, NOW), Err(AttError::UNLIKELY));
        assert_eq!(bridge.write(10, &[9], NOW), Err(AttError(0x05)));
        assert_eq!(bridge.write(10, &[9], NOW), Err(AttError::UNLIKELY));
    }

    #[test]
    fn rejects_unknown_handles() {
        let mut bridge = bridge([]);
        assert_eq!(bridge.read(99, 0, NOW), Err(AttError::INVALID_HANDLE));
        assert_eq!(bridge.read(21, 0, NOW), Err(AttError::INVALID_HANDLE));
        assert_eq!(bridge.write(99, &[1], NOW), Err(AttError::INVALID_HANDLE));
        assert!(bridge.device().ops.is_empty());
    }

    #[test]
    fn forwards_writes() {
        let mut bridge = bridge([]);
        assert_eq!(bridge.write(10, &[4, 5], NOW), Ok(()));
        assert_eq!(
            bridge.device().ops,
            [BleOp::Write {
                service: SERVICE,
                characteristic: version().characteristic,
                data: vec![4, 5]
            }]
        );
    }

    #[test]
    fn delivers_notifications_only_while_subscribed() {
        let mut bridge = bridge([]);
        bridge.device().notifications =
            [notification(status(), 1), notification(progress(), 2)].into();
        assert_eq!(bridge.poll(NOW), None);

        assert_eq!(bridge.write(21, &[1, 0], NOW), Ok(()));
        assert_eq!(bridge.write(31, &[2, 0], NOW), Ok(()));
        bridge.device().notifications = [
            notification(version(), 0),
            notification(status(), 1),
            notification(progress(), 2),
        ]
        .into();
        assert_eq!(
            bridge.poll(NOW),
            Some(Outgoing {
                handle: 20,
                kind: NotifyKind::Notification,
                data: vec![1]
            })
        );
        assert_eq!(
            bridge.poll(NOW),
            Some(Outgoing {
                handle: 30,
                kind: NotifyKind::Indication,
                data: vec![2]
            })
        );

        assert_eq!(bridge.write(21, &[0, 0], NOW), Ok(()));
        bridge.device().notifications = [notification(status(), 1)].into();
        assert_eq!(bridge.poll(NOW), None);
        assert_eq!(
            bridge.device().ops,
            [
                BleOp::Subscribe {
                    service: SERVICE,
                    characteristic: status().characteristic,
                    kind: NotifyKind::Notification
                },
                BleOp::Subscribe {
                    service: SERVICE,
                    characteristic: progress().characteristic,
                    kind: NotifyKind::Indication
                },
                BleOp::Unsubscribe {
                    service: SERVICE,
                    characteristic: status().characteristic
                },
            ]
        );
    }

    #[test]
    fn a_disconnect_drops_subscriptions() {
        let mut bridge = bridge([]);
        bridge.write(21, &[1, 0], NOW).unwrap();
        bridge.disconnect(NOW).unwrap();
        bridge.device().notifications = [notification(status(), 1)].into();
        assert_eq!(bridge.poll(NOW), None);
    }

    #[test]
    fn rejects_bad_cccd_writes() {
        let mut bridge = bridge([BleResult::Status(0x81)]);
        assert_eq!(bridge.write(21, &[1, 0], NOW), Err(AttError(0x81)));
        assert_eq!(bridge.write(21, &[1], NOW), Err(AttError::INVALID_LENGTH));
        assert_eq!(bridge.write(21, &[3, 0], NOW), Err(AttError::IMPROPER_CCCD));
        assert_eq!(bridge.device().ops.len(), 1);
        bridge.device().notifications = [notification(status(), 1)].into();
        assert_eq!(bridge.poll(NOW), None);
    }

    #[test]
    fn displays_att_errors() {
        assert_eq!(AttError(0x83).to_string(), "ATT error 0x83");
    }
}
