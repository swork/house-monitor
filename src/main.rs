#![no_std]
#![no_main]
#![allow(async_fn_in_trait)]

use core::str;

//use cyw43::Control;
use cyw43_pio::{ PioSpi, DEFAULT_CLOCK_DIVIDER };
use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH0, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_time::{Duration, Timer};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

#[embassy_executor::task]
async fn cyw43_task(
    runner: cyw43::Runner<'static,
    Output<'static>,
    PioSpi<'static,
            PIO0,
            0,
            DMA_CH0>>
    ) -> ! {
    runner.run().await
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let fw = include_bytes!("../cyw43-firmware/43439A0.bin");
    let clm = include_bytes!("../cyw43-firmware/43439A0_clm.bin");

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

    defmt::info!("cyw43 init...");

    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());
    let (_net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw).await;
    unwrap!(spawner.spawn(cyw43_task(runner)));

    defmt::info!("... done cyw43 init. Control init...");

    control.init(clm).await;
    control
        .set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;

    defmt::info!("... done Control init. Scan start...");
    // LED on during scan
    control.gpio_set(0, true).await;

    // In a block so scanner is released when done
    {
        let mut scanner = control.scan(Default::default()).await;
        while let Some(bss) = scanner.next().await {
            if let Ok(ssid_str) = str::from_utf8(&bss.ssid) {
                info!("scanned {} == {:x}", ssid_str, bss.bssid);
            }
        }
    }

    defmt::info!("... done Scan. Endless loop follows.");

    let blink_on_ms = Duration::from_millis(20);
    let blink_off_ms = Duration::from_secs(2);

    loop {
        // defmt::info!("Blink on");
        control.gpio_set(0, true).await;
        Timer::after(blink_on_ms).await;

        // defmt::info!("Blink off");
        control.gpio_set(0, false).await;
        Timer::after(blink_off_ms).await;
    }
}
