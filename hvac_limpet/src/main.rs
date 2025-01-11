#![no_std]
#![no_main]
#![allow(async_fn_in_trait)]

mod app;
/// Code structure here is patterned on https://github.com/embassy-rs/embassy,
/// examples/rp/src/wifi_webrequests.rs
mod secrets;
mod transport;

use core::str;
use cyw43::JoinOptions;
use cyw43_pio::{PioSpi, DEFAULT_CLOCK_DIVIDER};
use defmt::*;
use embassy_executor::Spawner;
use embassy_net::{Config, StackResources};
use embassy_rp::bind_interrupts;
use embassy_rp::clocks::RoscRng;
use embassy_rp::gpio::{Flex, Input, Level, Output, Pull};
use embassy_rp::peripherals::{DMA_CH0, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_time::Timer;
use rand_core::RngCore;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

#[embassy_executor::task]
async fn cyw43_task(
    runner: cyw43::Runner<'static, Output<'static>, PioSpi<'static, PIO0, 0, DMA_CH0>>,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
    runner.run().await
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    defmt::info!("main start");

    let p = embassy_rp::init(Default::default());
    let fw = include_bytes!("../cyw43-firmware/43439A0.bin");
    let clm = include_bytes!("../cyw43-firmware/43439A0_clm.bin");

    let mut rng = RoscRng;

    let pwr = Output::new(p.PIN_23, Level::Low);
    let cs = Output::new(p.PIN_25, Level::High);
    let mut pio = Pio::new(p.PIO0, Irqs);
    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        DEFAULT_CLOCK_DIVIDER,
        pio.irq0,
        cs,
        p.PIN_24,
        p.PIN_29,
        p.DMA_CH0,
    );

    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());
    let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw).await;
    unwrap!(spawner.spawn(cyw43_task(runner)));

    control.init(clm).await;
    control
        .set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;

    let secrets = secrets::get_secrets();

    // In a block so scanner is released when done
    {
        defmt::info!("scan");
        let mut scanner = control.scan(Default::default()).await;
        while let Some(bss) = scanner.next().await {
            if let Ok(ssid_str) = str::from_utf8(&bss.ssid) {
                info!("scanned {} == {:x}", ssid_str, bss.bssid);
            }
        }
    }

    let config = Config::dhcpv4(Default::default());
    // Use static IP configuration instead of DHCP
    //let config = embassy_net::Config::ipv4_static(embassy_net::StaticConfigV4 {
    //    address: Ipv4Cidr::new(Ipv4Address::new(192, 168, 69, 2), 24),
    //    dns_servers: Vec::new(),
    //    gateway: Some(Ipv4Address::new(192, 168, 69, 1)),
    //});

    // Generate random seed
    let seed = rng.next_u64();

    // Init network stack
    static RESOURCES: StaticCell<StackResources<5>> = StaticCell::new();
    let (stack, runner) = embassy_net::new(
        net_device,
        config,
        RESOURCES.init(StackResources::new()),
        seed,
    );

    unwrap!(spawner.spawn(net_task(runner)));

    loop {
        match control
            .join(
                secrets.wifi_ssid,
                JoinOptions::new(secrets.wifi_password.as_bytes()),
            )
            .await
        {
            Ok(_) => {
                defmt::info!("joined wifi {}", secrets.wifi_ssid);
                break;
            }
            Err(err) => {
                info!(
                    "join {} failed with status={}",
                    secrets.wifi_ssid, err.status
                );
            }
        }
    }

    // Wait for DHCP, not necessary when using static IP
    info!("waiting for DHCP...");
    while !stack.is_config_up() {
        Timer::after_millis(100).await;
    }
    info!("DHCP is now up!");

    info!("waiting for link up...");
    while !stack.is_link_up() {
        Timer::after_millis(500).await;
    }
    info!("Link is up!");

    info!("waiting for stack to be up...");
    stack.wait_config_up().await;
    info!("Stack is up!");

    // I'd like to let app.rs specify how to assign pins,
    // rather than coordinate, but haven't figured it out yet.
    // Nor have figured out how/if p can be passed there.
    static INPUTS: StaticCell<app::InputPinsMonitoringLeds> = StaticCell::new();
    static PIN_2_STATIC: StaticCell<Input> = StaticCell::new();
    static PIN_3_STATIC: StaticCell<Input> = StaticCell::new();
    static PIN_6_STATIC: StaticCell<Input> = StaticCell::new();
    static PIN_7_STATIC: StaticCell<Input> = StaticCell::new();
    static PIN_8_STATIC: StaticCell<Input> = StaticCell::new();
    static PIN_9_STATIC: StaticCell<Input> = StaticCell::new();
    static PIN_10_STATIC: StaticCell<Input> = StaticCell::new();
    static PIN_11_STATIC: StaticCell<Input> = StaticCell::new();
    //static PIN_12_STATIC: StaticCell<Flex> = StaticCell::new();
    let inputs: &'static mut app::InputPinsMonitoringLeds = INPUTS.init_with(|| app::InputPinsMonitoringLeds {
        heat: PIN_2_STATIC.init_with(|| Input::new(p.PIN_2, Pull::Down)),
        cool: PIN_3_STATIC.init_with(|| Input::new(p.PIN_3, Pull::Down)),
        emergency: PIN_6_STATIC.init_with(|| Input::new(p.PIN_6, Pull::Down)),
        purge: PIN_7_STATIC.init_with(|| Input::new(p.PIN_7, Pull::Down)),
        zone1: PIN_8_STATIC.init_with(|| Input::new(p.PIN_8, Pull::Down)),
        zone2: PIN_9_STATIC.init_with(|| Input::new(p.PIN_9, Pull::Down)),
        zone3: PIN_10_STATIC.init_with(|| Input::new(p.PIN_10, Pull::Down)),
        zone4: PIN_11_STATIC.init_with(|| Input::new(p.PIN_11, Pull::Down)),
        //onewire: PIN_12_STATIC.init_with(|| Flex::new(p.PIN_12)),
    });

    app::run(spawner, inputs, stack, secrets, seed).await; // never returns
}
