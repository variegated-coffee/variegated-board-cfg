# Variegated Board Cfg

Substantial credits go to James Munns for the [toml-cfg](https://github.com/jamesmunns/toml-cfg) crate, Adam Greig for the [assign-resources](https://github.com/adamgreig/assign-resources) crate, and Adin Ackerman for [the procedural overhaul PR for assign-resources](https://github.com/adamgreig/assign-resources/pull/11).

## Example configuration

### lib.rs

```rust
#[variegated_board_cfg::config_section("hid_bus")]
struct HidBus {
    tx_pin: (),
    rx_pin: impl embassy_rp::peripherals::Pin, // Forces a compile error if the type of rx_pin doesn't implement Pin
    uart: (),
    baud_rate: u32,
}

```

### board-config.toml

```toml
[hid_bus]
tx_pin = "embassy_rp::peripherals::PIN_0"
rx_pin = "embassy_rp::peripherals::PIN_1"
uart = "embassy_rp::peripherals::UART0"
baud_rate = 115200
```

## Expansion

```rust
type HidBusTxPin = embassy_rp::peripherals::PIN_0;
type HidBusRxPin = embassy_rp::peripherals::PIN_1;
type HidBusUart = embassy_rp::peripherals::UART0;

struct HidBus {
    tx_pin: HidBusTxPin,
    rx_pin: HidBusRxPin,
    uart: HidBusUart,
    baud_rate: u32,
}

impl HidBus where HidBusRxPin: embassy_rp::peripherals::Pin {

}

macro_rules! hid_bus {
    ($P : ident) => {
        HidBus {
            tx_pin: $P.PIN_0,
            rx_pin: $P.PIN_1,
            uart: $P.UART0,
            baud_rate: 115200
        }
    };
}

```