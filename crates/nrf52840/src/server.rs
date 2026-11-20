use core::cell::RefCell;

use embassy_futures::select::select;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer};
use nrf_softdevice::ble::gatt_server::{self, WriteOp};
use nrf_softdevice::ble::{Connection, DeferredReadReply, DeferredWriteReply, GattError};
use pump_shared::{Address, NotifyKind, Timestamp};
use simulator_bridge::{AttError, Attributes, Bridge, Command, Simulator};

use crate::device::Device;

const POLL: Duration = Duration::from_millis(50);

pub struct Server {
    bridge: Mutex<CriticalSectionRawMutex, RefCell<Bridge<Device>>>,
    wake: Signal<CriticalSectionRawMutex, ()>,
    drop_link: Signal<CriticalSectionRawMutex, ()>,
}

impl Server {
    pub fn new(device: Device, attributes: Attributes) -> Self {
        Self {
            bridge: Mutex::new(RefCell::new(Bridge::new(device, attributes))),
            wake: Signal::new(),
            drop_link: Signal::new(),
        }
    }

    pub fn connect(&self, address: Address) {
        let _ = self.event(|bridge, now| bridge.connect(address, now));
    }

    pub fn disconnect(&self) {
        let _ = self.event(|bridge, now| bridge.disconnect(now));
    }

    pub fn apply(&self, command: Command) {
        self.event(|bridge, now| {
            let now = bridge.shifted(now);
            bridge.device().apply(command, now);
        });
        if command == Command::Reboot {
            self.drop_link.signal(());
        }
    }

    pub fn advance(&self, millis: u64) {
        self.event(|bridge, _now| bridge.advance(millis));
    }

    pub fn reset(&self) {
        self.event(|bridge, _now| {
            *bridge.device() = crate::device::new();
            bridge.reset();
        });
        self.drop_link.signal(());
    }

    pub async fn dropped(&self) {
        self.drop_link.reset();
        self.drop_link.wait().await
    }

    pub async fn notify(&self, conn: &Connection) {
        loop {
            select(self.wake.wait(), Timer::after(POLL)).await;
            while let Some(out) = self.with(|bridge, now| bridge.poll(now)) {
                let _ = match out.kind {
                    NotifyKind::Notification => {
                        gatt_server::notify_value(conn, out.handle, &out.data).is_ok()
                    }
                    NotifyKind::Indication => {
                        gatt_server::indicate_value(conn, out.handle, &out.data).is_ok()
                    }
                };
            }
        }
    }

    fn with<T>(&self, f: impl FnOnce(&mut Bridge<Device>, Timestamp) -> T) -> T {
        let now = Timestamp::from_millis(Instant::now().as_millis());
        self.bridge.lock(|bridge| f(&mut bridge.borrow_mut(), now))
    }

    fn event<T>(&self, f: impl FnOnce(&mut Bridge<Device>, Timestamp) -> T) -> T {
        let result = self.with(f);
        self.wake.signal(());
        result
    }
}

impl gatt_server::Server for Server {
    type Event = ();

    fn on_write(
        &self,
        _conn: &Connection,
        handle: u16,
        _op: WriteOp,
        _offset: usize,
        data: &[u8],
    ) -> Option<()> {
        let _ = self.event(|bridge, now| bridge.write(handle, data, now));
        None
    }

    fn on_deferred_read(&self, handle: u16, offset: usize, reply: DeferredReadReply) -> Option<()> {
        let value = self.event(|bridge, now| bridge.read(handle, offset, now));
        let _ = reply.reply(value.as_deref().map(Some).map_err(|&e| gatt_error(e)));
        None
    }

    fn on_deferred_write(
        &self,
        handle: u16,
        _op: WriteOp,
        _offset: usize,
        data: &[u8],
        reply: DeferredWriteReply,
    ) -> Option<()> {
        let result = self.event(|bridge, now| bridge.write(handle, data, now));
        let _ = reply.reply(result.map(|()| data).map_err(gatt_error));
        None
    }
}

fn gatt_error(error: AttError) -> GattError {
    GattError::from_att_error(error.0)
}
