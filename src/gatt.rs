use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

use pump_shared::{GattChar, Uuid};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    pub uuid: Uuid,
    pub characteristics: Vec<GattChar>,
}

pub fn services(table: impl IntoIterator<Item = GattChar>) -> Vec<Service> {
    let mut services: Vec<Service> = Vec::new();
    for characteristic in table {
        match services
            .iter_mut()
            .find(|s| s.uuid == characteristic.service)
        {
            Some(service) => service.characteristics.push(characteristic),
            None => services.push(Service {
                uuid: characteristic.service,
                characteristics: vec![characteristic],
            }),
        }
    }
    services
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attribute {
    Value(GattChar),
    Cccd {
        characteristic: GattChar,
        value: u16,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attributes(BTreeMap<u16, Attribute>);

impl Attributes {
    pub fn bind(&mut self, characteristic: GattChar, value: u16, cccd: Option<u16>) {
        self.0.insert(value, Attribute::Value(characteristic));
        if let Some(cccd) = cccd {
            self.0.insert(
                cccd,
                Attribute::Cccd {
                    characteristic,
                    value,
                },
            );
        }
    }

    pub fn get(&self, handle: u16) -> Option<Attribute> {
        self.0.get(&handle).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pump_shared::NotifyKind;

    const A: Uuid = Uuid::from_bytes([1; 16]);
    const B: Uuid = Uuid::from_bytes([2; 16]);

    fn chr(service: Uuid, byte: u8, notify: Option<NotifyKind>) -> GattChar {
        GattChar {
            name: "c",
            service,
            characteristic: Uuid::from_bytes([byte; 16]),
            notify,
        }
    }

    #[test]
    fn groups_characteristics_by_service_in_declaration_order() {
        let table = [chr(A, 10, None), chr(B, 11, None), chr(A, 12, None)];
        let grouped = services(table);
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[0].uuid, A);
        assert_eq!(grouped[0].characteristics, [table[0], table[2]]);
        assert_eq!(grouped[1].characteristics, [table[1]]);
    }

    #[test]
    fn resolves_value_and_cccd_handles() {
        let plain = chr(A, 10, None);
        let notifying = chr(A, 11, Some(NotifyKind::Notification));
        let mut attributes = Attributes::default();
        attributes.bind(plain, 3, None);
        attributes.bind(notifying, 5, Some(6));
        assert_eq!(attributes.get(3), Some(Attribute::Value(plain)));
        assert_eq!(attributes.get(5), Some(Attribute::Value(notifying)));
        assert_eq!(
            attributes.get(6),
            Some(Attribute::Cccd {
                characteristic: notifying,
                value: 5
            })
        );
        assert_eq!(attributes.get(4), None);
    }
}
