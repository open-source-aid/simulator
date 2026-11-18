use alloc::format;

use embassy_futures::join::join;
use embassy_futures::select::{select, Either};
use embassy_nrf::bind_interrupts;
use embassy_nrf::interrupt::{self, InterruptExt, Priority};
use embassy_nrf::peripherals::USBD;
use embassy_nrf::usb::vbus_detect::SoftwareVbusDetect;
use embassy_nrf::usb::{self, Driver};
use embassy_usb::class::cdc_acm::{CdcAcmClass, Sender, State};
use embassy_usb::{Builder, Config};
use nrf_softdevice::raw;
use simulator_bridge::{Command, Lines};
use static_cell::StaticCell;

use crate::server::Server;

const PACKET: usize = 64;
const TOUCH_BAUD: u32 = 1200;
const UF2_MAGIC: u32 = 0x57;

bind_interrupts!(struct Irqs {
    USBD => usb::InterruptHandler<USBD>;
});

type UsbDriver = Driver<'static, USBD, &'static SoftwareVbusDetect>;

#[embassy_executor::task]
pub async fn run(usbd: USBD, server: &'static Server) {
    interrupt::USBD.set_priority(Priority::P2);
    static VBUS: StaticCell<SoftwareVbusDetect> = StaticCell::new();
    let driver = Driver::new(usbd, Irqs, &*VBUS.init(SoftwareVbusDetect::new(true, true)));
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
    let mut device = builder.build();
    join(device.run(), serve(class, server)).await;
}

async fn serve(class: CdcAcmClass<'static, UsbDriver>, server: &Server) {
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
            Either::Second(()) if rx.line_coding().data_rate() == TOUCH_BAUD => bootloader(),
            Either::Second(()) => {}
        }
    }
}

async fn write(tx: &mut Sender<'static, UsbDriver>, bytes: &[u8]) {
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
    unsafe { raw::sd_power_gpregret_set(0, UF2_MAGIC) };
    cortex_m::peripheral::SCB::sys_reset()
}
