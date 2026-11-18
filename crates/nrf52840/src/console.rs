use alloc::format;
use core::ptr;

use embassy_futures::join::join;
use embassy_futures::select::{select, Either};
use embassy_nrf::bind_interrupts;
use embassy_nrf::interrupt::{self, InterruptExt, Priority};
use embassy_nrf::peripherals::USBD;
use embassy_nrf::usb::vbus_detect::{self, HardwareVbusDetect, SoftwareVbusDetect, VbusDetect};
use embassy_nrf::usb::{self, Driver};
use embassy_time::Timer;
use embassy_usb::class::cdc_acm::{CdcAcmClass, Receiver, Sender, State};
use embassy_usb::{Builder, Config, UsbDevice};
use nrf_softdevice::raw;
use simulator_bridge::{Command, Lines};
use static_cell::StaticCell;

use crate::server::Server;

const PACKET: usize = 64;
const TOUCH_BAUD: u32 = 1200;
const UF2_MAGIC: u32 = 0x57;
const GPREGRET: *mut u32 = 0x4000_051C as _;

bind_interrupts!(struct Irqs {
    USBD => usb::InterruptHandler<USBD>;
    CLOCK_POWER => vbus_detect::InterruptHandler;
});

type Usb<V> = Driver<'static, USBD, V>;

#[embassy_executor::task]
pub async fn run(usbd: USBD, vbus: &'static SoftwareVbusDetect, server: &'static Server) {
    let (mut device, class) = usb(usbd, vbus);
    join(device.run(), serve(class, server)).await;
}

#[embassy_executor::task]
pub async fn report(usbd: USBD, text: &'static str) {
    let (mut device, class) = usb(usbd, HardwareVbusDetect::new(Irqs));
    join(device.run(), repeat(class, text)).await;
}

fn usb<V: VbusDetect + 'static>(
    usbd: USBD,
    vbus: V,
) -> (UsbDevice<'static, Usb<V>>, CdcAcmClass<'static, Usb<V>>) {
    interrupt::USBD.set_priority(Priority::P2);
    let driver = Driver::new(usbd, Irqs, vbus);
    let mut config = Config::new(0x2886, 0x8045);
    config.manufacturer = Some("u-osaid");
    config.product = Some("simulator-nrf52840");
    static CONFIG: StaticCell<[u8; 256]> = StaticCell::new();
    static BOS: StaticCell<[u8; 256]> = StaticCell::new();
    static CONTROL: StaticCell<[u8; 64]> = StaticCell::new();
    static STATE: StaticCell<State> = StaticCell::new();
    let mut builder = Builder::new(
        driver,
        config,
        CONFIG.init([0; 256]),
        BOS.init([0; 256]),
        &mut [],
        CONTROL.init([0; 64]),
    );
    let class = CdcAcmClass::new(&mut builder, STATE.init(State::new()), PACKET as u16);
    (builder.build(), class)
}

async fn serve<V: VbusDetect + 'static>(class: CdcAcmClass<'static, Usb<V>>, server: &Server) {
    let (mut tx, mut rx, control) = class.split_with_control();
    let mut lines = Lines::default();
    let mut packet = [0; PACKET];
    loop {
        match select(rx.read_packet(&mut packet), control.control_changed()).await {
            Either::First(Ok(len)) => {
                for line in packet[..len].iter().filter_map(|&byte| lines.push(byte)) {
                    let reply = match line.parse::<Command>() {
                        Ok(command) => {
                            server.apply(command);
                            format!("ok: {}\r\n", line.trim())
                        }
                        Err(error) => format!("{error}\r\n"),
                    };
                    write(&mut tx, reply.as_bytes()).await;
                }
            }
            Either::First(Err(_)) => rx.wait_connection().await,
            Either::Second(()) => touch(&rx),
        }
    }
}

async fn repeat<V: VbusDetect + 'static>(class: CdcAcmClass<'static, Usb<V>>, text: &str) {
    let (mut tx, rx, control) = class.split_with_control();
    loop {
        let print = async {
            write(&mut tx, b"panic: ").await;
            write(&mut tx, text.as_bytes()).await;
            write(&mut tx, b"\r\n").await;
            Timer::after_secs(1).await;
        };
        if let Either::Second(()) = select(print, control.control_changed()).await {
            touch(&rx);
        }
    }
}

fn touch<V: VbusDetect + 'static>(rx: &Receiver<'static, Usb<V>>) {
    if rx.line_coding().data_rate() == TOUCH_BAUD {
        bootloader()
    }
}

async fn write<V: VbusDetect + 'static>(tx: &mut Sender<'static, Usb<V>>, bytes: &[u8]) {
    for chunk in bytes.chunks(PACKET) {
        if tx.write_packet(chunk).await.is_err() {
            return;
        }
    }
    if bytes.len() % PACKET == 0 {
        let _ = tx.write_packet(&[]).await;
    }
}

fn bootloader() -> ! {
    let mut enabled = 0;
    unsafe {
        raw::sd_softdevice_is_enabled(&mut enabled);
        if enabled == 0 {
            ptr::write_volatile(GPREGRET, UF2_MAGIC);
        } else {
            raw::sd_power_gpregret_set(0, UF2_MAGIC);
        }
    }
    cortex_m::peripheral::SCB::sys_reset()
}
