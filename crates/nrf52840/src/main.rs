#![no_std]
#![no_main]

extern crate alloc;

mod console;
mod device;
mod led;
mod report;
mod server;

use core::mem::MaybeUninit;
use core::panic::PanicInfo;
use core::ptr::addr_of_mut;
use embassy_time::Timer;

use cortex_m_rt::{exception, ExceptionFrame};
use embassy_executor::Spawner;
use embassy_futures::select::select3;
use embassy_nrf::config::HfclkSource;
use embassy_nrf::interrupt::Priority;
use embassy_nrf::usb::vbus_detect::SoftwareVbusDetect;
use embedded_alloc::LlffHeap;
use nrf_softdevice::ble::advertisement_builder::{Flag, LegacyAdvertisementBuilder, ServiceList};
use nrf_softdevice::ble::gatt_server::builder::ServiceBuilder;
use nrf_softdevice::ble::gatt_server::characteristic::{Attribute, Metadata, Properties};
use nrf_softdevice::ble::{gatt_server, peripheral, Uuid};
use nrf_softdevice::{raw, SocEvent, Softdevice};
use pump_shared::{Address, NotifyKind};
use simulator_bridge::{Attributes, Service, Simulator};
use static_cell::StaticCell;

use crate::server::Server;

const HEAP_SIZE: usize = 64 * 1024;
const MAX_VALUE: u16 = 244;
const VBUS_DETECTED: u32 = 1;
const VBUS_READY: u32 = 2;

#[global_allocator]
static HEAP: LlffHeap = LlffHeap::empty();

#[embassy_executor::task]
async fn softdevice(sd: &'static Softdevice, vbus: &'static SoftwareVbusDetect) -> ! {
    sd.run_with_callback(|event| match event {
        SocEvent::PowerUsbDetected | SocEvent::PowerUsbRemoved | SocEvent::PowerUsbPowerReady => {
            track(vbus)
        }
        _ => {}
    })
    .await
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    report::save(info)
}

#[exception]
unsafe fn HardFault(frame: &ExceptionFrame) -> ! {
    report::save(format_args!("hard fault at pc {:#010x}", frame.pc()))
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    static mut MEMORY: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
    unsafe { HEAP.init(addr_of_mut!(MEMORY) as usize, HEAP_SIZE) }

    let report = report::stored();
    let mut config = embassy_nrf::config::Config::default();
    config.gpiote_interrupt_priority = Priority::P2;
    config.time_interrupt_priority = Priority::P2;
    if report.is_some() {
        config.hfclk_source = HfclkSource::ExternalXtal;
    }
    let peripherals = embassy_nrf::init(config);
    if let Some(text) = report {
        spawner.must_spawn(console::report(peripherals.USBD, text));
        led::blink(led::RED).await
    }
    led::show(led::BLUE);

    let device = device::new();
    let name = device.name();
    let services = device.services();
    let advertised = uuid_bytes(device.advertised());
    let sd = Softdevice::enable(&softdevice_config(&services));
    let attributes = register(sd, &services);
    static SERVER: StaticCell<Server> = StaticCell::new();
    let server = SERVER.init(Server::new(device, attributes));
    static VBUS: StaticCell<SoftwareVbusDetect> = StaticCell::new();
    let vbus = &*VBUS.init(SoftwareVbusDetect::new(false, false));
    unsafe {
        raw::sd_power_usbdetected_enable(1);
        raw::sd_power_usbremoved_enable(1);
        raw::sd_power_usbpwrrdy_enable(1);
    }
    track(vbus);
    unsafe { raw::sd_clock_hfclk_request() };
    spawner.must_spawn(softdevice(sd, vbus));
    spawner.must_spawn(console::run(peripherals.USBD, vbus, server));
    led::show(led::GREEN);
    report::clear();

    let adv_data = LegacyAdvertisementBuilder::new()
        .flags(&[Flag::GeneralDiscovery, Flag::LE_Only])
        .services_128(ServiceList::Incomplete, &[advertised])
        .build();
    let scan_data = LegacyAdvertisementBuilder::new().full_name(&name).build();
    loop {
        let advertisement = peripheral::ConnectableAdvertisement::ScannableUndirected {
            adv_data: &adv_data,
            scan_data: &scan_data,
        };
        let config = peripheral::Config::default();
        let Ok(conn) = peripheral::advertise_connectable(sd, advertisement, &config).await else {
            Timer::after_millis(100).await;
            continue;
        };
        let mut peer = conn.peer_address().bytes();
        peer.reverse();
        server.connect(Address::new(peer));
        select3(
            gatt_server::run(&conn, server, |()| {}),
            server.notify(&conn),
            server.dropped(),
        )
        .await;
        let _ = conn.disconnect();
        server.disconnect();
    }
}

fn track(vbus: &SoftwareVbusDetect) {
    let mut status = 0;
    unsafe { raw::sd_power_usbregstatus_get(&mut status) };
    vbus.detected(status & VBUS_DETECTED != 0);
    if status & VBUS_READY != 0 {
        vbus.ready();
    }
}

fn uuid_bytes(uuid: pump_shared::Uuid) -> [u8; 16] {
    let mut bytes = *uuid.as_bytes();
    bytes.reverse();
    bytes
}

fn register(sd: &mut Softdevice, services: &[Service]) -> Attributes {
    let mut attributes = Attributes::default();
    for service in services {
        let uuid = Uuid::new_128(&uuid_bytes(service.uuid));
        let mut builder = ServiceBuilder::new(sd, uuid).expect("service registers");
        for characteristic in &service.characteristics {
            let properties = Properties::new().read().write().write_without_response();
            let properties = match characteristic.notify {
                None => properties,
                Some(NotifyKind::Notification) => properties.notify(),
                Some(NotifyKind::Indication) => properties.indicate(),
            };
            let value = Attribute::new([0u8; 0]).variable_len(MAX_VALUE).deferred();
            let uuid = Uuid::new_128(&uuid_bytes(characteristic.characteristic));
            let handles = builder
                .add_characteristic(uuid, value, Metadata::new(properties))
                .expect("characteristic registers")
                .build();
            let cccd = characteristic.notify.map(|_| handles.cccd_handle);
            attributes.bind(*characteristic, handles.value_handle, cccd);
        }
        builder.build();
    }
    attributes
}

fn softdevice_config(services: &[Service]) -> nrf_softdevice::Config {
    let uuids = services
        .iter()
        .map(|s| 1 + s.characteristics.len())
        .sum::<usize>();
    nrf_softdevice::Config {
        clock: Some(raw::nrf_clock_lf_cfg_t {
            source: raw::NRF_CLOCK_LF_SRC_XTAL as u8,
            rc_ctiv: 0,
            rc_temp_ctiv: 0,
            accuracy: raw::NRF_CLOCK_LF_ACCURACY_20_PPM as u8,
        }),
        conn_gap: Some(raw::ble_gap_conn_cfg_t {
            conn_count: 1,
            event_length: 24,
        }),
        conn_gatt: Some(raw::ble_gatt_conn_cfg_t {
            att_mtu: MAX_VALUE + 3,
        }),
        common_vs_uuid: Some(raw::ble_common_cfg_vs_uuid_t {
            vs_uuid_count: uuids as u8,
        }),
        gatts_attr_tab_size: Some(raw::ble_gatts_cfg_attr_tab_size_t {
            attr_tab_size: 16 * 1024,
        }),
        gap_role_count: Some(raw::ble_gap_cfg_role_count_t {
            adv_set_count: 1,
            periph_role_count: 1,
            central_role_count: 0,
            central_sec_count: 0,
            _bitfield_1: raw::ble_gap_cfg_role_count_t::new_bitfield_1(0),
        }),
        ..Default::default()
    }
}
