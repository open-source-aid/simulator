# simulator-nrf52840

A physical BLE peripheral that impersonates a medical device (insulin pump now, CGM later) so a
real phone app (AndroidAPS, Loop, Trio) can be tested end to end against a simulator.

Any `pump_shared::SimDevice` that also implements `simulator_nrf52840::Simulator` (name,
advertised service, GATT table, scenario knobs) plugs in. The first one is the YpsoPump simulator
`YpsoSim` from `pump-ypso`.

Target: Seeed XIAO nRF52840 (Sense), S140 7.3.0 SoftDevice and the Adafruit UF2 bootloader that
ship on the board; Embassy + `nrf-softdevice`, `no_std` + `alloc`.

## Layout

- `src/` host-testable, `no_std` library `simulator-nrf52840`
  - `bridge.rs` GATT peripheral events to `SimDevice` operations
  - `gatt.rs` services from a `GattChar` table, attribute handle map
  - `console.rs` serial console commands and line splitting
  - `simulator.rs` the `Simulator` trait a device implements to be hosted
  - `ypso.rs` YpsoPump GATT table, name, and `Simulator` for `YpsoSim` (feature `ypso-sim`)
- `firmware/` the nRF52840 binary (separate crate, cross-compiled)
- `tools/uf2.py` binary to UF2 converter, `tools/check.swift` macOS BLE central check

## Bridge mapping

| Peripheral event | Simulator call | Answer to the central |
| --- | --- | --- |
| central connected | `BleOp::Connect { address }` | |
| central disconnected | `BleOp::Disconnect`, subscriptions dropped | |
| read of a value | `BleOp::Read` | `Data(v)` is the value, `Status(c)` ATT error `c`, anything else `0x0E` |
| read continuation (offset > 0) | none, served from the first answer | `0x07` if out of range |
| write of a value | `BleOp::Write` | `Done` success, `Status(c)` ATT error `c`, anything else `0x0E` |
| CCCD write `0x0001` / `0x0002` | `BleOp::Subscribe { kind }` | `0xFD` if the kind is not the one declared |
| CCCD write `0x0000` | `BleOp::Unsubscribe` | |
| every 50 ms and after every event | `poll_notification(now)` | sent as notification or indication if subscribed, dropped otherwise |

`now` is the Embassy monotonic clock in milliseconds since boot.

## Host checks

```sh
cargo test --all-features
cargo llvm-cov --all-features
cargo fmt --check
cargo clippy --all-features --all-targets -- -D warnings
```

## Firmware

```sh
rustup target add thumbv7em-none-eabihf
cd firmware
cargo build --release
```

The device is the `YpsoSim` from `pump-ypso`:

- `opn` (default): `YpsoSim::opn`, clear text
- `--no-default-features`: `YpsoSim::new`, encrypted

It advertises the YpsoPump service UUID `669A0C20-0008-969E-E211-FCFF00000002`.

RAM from `0x20018000` (160 KB, the SoftDevice keeps what is below): 64 KB heap, 32 KB task arena,
about 61 KB stack.

Flash through the UF2 bootloader:

```sh
B=$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/host: //p')/bin
$B/llvm-objcopy -O binary target/thumbv7em-none-eabihf/release/simulator-nrf52840 target/simulator-nrf52840.bin
python3 ../tools/uf2.py target/simulator-nrf52840.bin 0x27000 target/simulator-nrf52840.uf2
cp target/simulator-nrf52840.uf2 /Volumes/XIAO-SENSE/
```

The `XIAO-SENSE` drive appears after a double-tap of the reset button. Once this firmware runs,
opening its USB serial port at 1200 baud reboots it into the bootloader, so later flashes need no
button.

## Console

The USB serial port (any baud) accepts one command per line and answers `ok: <command>` or an
error:

- `battery <n>` raw battery field
- `reservoir <n>` reservoir in device units (YpsoPump: hundredths of a unit)
- `alarm battery|reservoir|occlusion` raises the alarm and stops delivery
- `acknowledge` clears the active alarm
- `stop` / `run` stops or resumes delivery
- `reboot` reboots the simulated device and drops the BLE link

## Check from a Mac

```sh
swift tools/check.swift YpsoPump_10155555
```

Scans, connects, and reads the pump base service version (`1.1` clear text, `1.2` encrypted).

## Licence

AGPL-3.0-or-later.
