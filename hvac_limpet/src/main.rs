#![no_std]
#![no_main]
#![allow(async_fn_in_trait)]

/// This implementation of hvac_limpet runs on a Raspberry Pi Pico W
/// microcontroller board. It monitors my Honeywell TZ-4 heat pump control
/// panel and reports state changes to a time-series database. Network 
/// interactions are patterned on embassy/examples/rp/src/wifi_webrequests.rs

use core::str;
use cyw43::JoinOptions;
use cyw43_pio::{PioSpi, DEFAULT_CLOCK_DIVIDER};
use defmt::*;
use embassy_executor::Spawner;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_net::{Config, StackResources};
use embassy_rp::bind_interrupts;
use embassy_rp::clocks::RoscRng;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH0, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_time::{Duration, Timer};
use rand_core::RngCore;
use reqwless::client::{HttpClient, TlsConfig, TlsVerify};
use reqwless::request::Method;
use static_cell::{ConstStaticCell, StaticCell};
use {defmt_rtt as _, panic_probe as _};

mod secrets;
use crate::secrets::get_secrets;

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

    let secrets = get_secrets();

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
                break
            },
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

    // And now we can use it!

    static RX_BUFFER: ConstStaticCell<[u8; 4192]> = ConstStaticCell::new([0; 4192]);
    let rx_buffer: &'static mut [u8; 4192] = RX_BUFFER.take();

    static TLS_READ_BUFFER: ConstStaticCell<[u8; 16640]> = ConstStaticCell::new([0; 16640]);
    let tls_read_buffer: &'static mut [u8; 16640] = TLS_READ_BUFFER.take();

    static TLS_WRITE_BUFFER: ConstStaticCell<[u8; 16640]> = ConstStaticCell::new([0; 16640]);
    let tls_write_buffer: &'static mut [u8; 16640] = TLS_WRITE_BUFFER.take();

    loop {
        let client_state = TcpClientState::<1, 1024, 1024>::new();
        let tcp_client = TcpClient::new(stack, &client_state);
        let dns_client = DnsSocket::new(stack);
        let tls_config = TlsConfig::new(seed, tls_read_buffer, tls_write_buffer, TlsVerify::None);

        let mut http_client = HttpClient::new_with_tls(&tcp_client, &dns_client, tls_config);
        //        let mut http_client = HttpClient::new(&tcp_client, &dns_client);
        let url = secrets.couchdb_url;
        // for non-TLS requests, use this instead:
        // let mut http_client = HttpClient::new(&tcp_client, &dns_client);
        // let url = "http://worldtimeapi.org/api/timezone/Europe/Berlin";

        info!("connecting to {}", &url);

        let mut request = match http_client.request(Method::GET, &url).await {
            Ok(req) => req,
            Err(e) => {
                error!("Failed to make HTTP request: {:?}", e);
                return; // handle the error
            }
        };

        let response = match request.send(rx_buffer).await {
            Ok(resp) => resp,
            Err(_e) => {
                error!("Failed to send HTTP request");
                return; // handle the error;
            }
        };

        let body = match str::from_utf8(response.body().read_to_end().await.unwrap()) {
            Ok(b) => b,
            Err(_e) => {
                error!("Failed to read response body");
                return; // handle the error
            }
        };
        info!("Response body: {:?}", &body);

        Timer::after(Duration::from_secs(5)).await;
    }
}