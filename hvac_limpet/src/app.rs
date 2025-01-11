use crate::secrets::Secrets;
use cyw43::Control;

use core::str;
use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::select::select_slice;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_net::Stack;
use embassy_rp::gpio::{Flex, Input};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_time::{Duration, Timer};
use heapless::Vec;
use reqwless::client::{HttpClient, TlsConfig, TlsVerify};
use reqwless::request::Method;
use static_cell::{ConstStaticCell, StaticCell};

pub struct IoPins<'a> {
    pub heat: &'a mut Input<'a>,
    pub cool: &'a mut Input<'a>,
    pub emergency: &'a mut Input<'a>,
    pub purge: &'a mut Input<'a>,
    pub zone1: &'a mut Input<'a>,
    pub zone2: &'a mut Input<'a>,
    pub zone3: &'a mut Input<'a>,
    pub zone4: &'a mut Input<'a>,
    pub onewire: &'a mut Flex<'a>,
}

enum TriggerMessage {
    Trigger,
}

type TriggerChannel = Channel<NoopRawMutex, TriggerMessage, 10>;

#[embassy_executor::task]
pub async fn monitor_inputs(
    funnel: Sender<'static, NoopRawMutex, TriggerMessage, 10>,
    mut triggers: Vec<&'static mut Input<'static>, 8>,
) -> ! {
    loop {
        let mut futures = Vec::<_, 8>::new();
        for input in triggers.as_mut_slice() {
            let wait_for_an_edge = input.wait_for_any_edge();
            let r = futures.push(wait_for_an_edge);
            if let Err(_) = r {
                defmt::panic!("too many pushes to heapless::Vec of futures. (How?)");
            }
        }
        select_slice(futures.as_mut_slice()).await;
        match funnel.try_send(TriggerMessage::Trigger) {
            Ok(()) => {}
            Err(_) => {
                error!("An input changed state but the TriggerMessage queue is full.")
            }
        }

        // Several LEDs change state at "the same time". One will be first.
        // Let the others settle before checking again - saves sending several
        // quick messages unnecessarily.
        Timer::after_secs(2).await;
    }
}

pub async fn run(
    spawner: Spawner,
    io_pins: &'static mut IoPins<'static>,
    _control: Control<'_>,
    stack: Stack<'_>,
    secrets: Secrets<'_>,
    seed: u64,
) -> ! {
    static RX_BUFFER: ConstStaticCell<[u8; 4192]> = ConstStaticCell::new([0; 4192]);
    let rx_buffer: &'static mut [u8; 4192] = RX_BUFFER.take();

    static TLS_READ_BUFFER: ConstStaticCell<[u8; 16640]> = ConstStaticCell::new([0; 16640]);
    let tls_read_buffer: &'static mut [u8; 16640] = TLS_READ_BUFFER.take();

    static TLS_WRITE_BUFFER: ConstStaticCell<[u8; 16640]> = ConstStaticCell::new([0; 16640]);
    let tls_write_buffer: &'static mut [u8; 16640] = TLS_WRITE_BUFFER.take();

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

    let mut triggers = Vec::<&'static mut Input, 8>::new();
    let _ = triggers.push(io_pins.heat);
    let _ = triggers.push(io_pins.cool);
    let _ = triggers.push(io_pins.purge);
    let _ = triggers.push(io_pins.emergency);
    let _ = triggers.push(io_pins.zone1);
    let _ = triggers.push(io_pins.zone2);
    let _ = triggers.push(io_pins.zone3);
    let oops = triggers.push(io_pins.zone4);
    if let Err(_) = oops {
        defmt::panic!("Too many pushes to heapless::Vec (how?)");
    }

    // 10 deep, 2s between possible sends leaves 20s for blocked network send
    // before errors accumulate. Be sure queuing errors are handled robustly.
    static TRIGGER_CHANNEL: StaticCell<TriggerChannel> = StaticCell::new();
    let trigger_channel = TRIGGER_CHANNEL.init(TriggerChannel::new());

    unwrap!(spawner.spawn(monitor_inputs(trigger_channel.sender(), triggers)));

    loop {
        info!("connecting to {}", &url);

        let mut request = match http_client.request(Method::GET, &url).await {
            Ok(req) => req,
            Err(e) => {
                error!("Failed to make HTTP request: {}", e);
                // cope
                continue;
            }
        };

        let response = match request.send(rx_buffer).await {
            Ok(resp) => resp,
            Err(_e) => {
                error!("Failed to send HTTP request");
                // cope
                continue;
            }
        };

        let body = match str::from_utf8(response.body().read_to_end().await.unwrap()) {
            Ok(b) => b,
            Err(_e) => {
                error!("Failed to read response body");
                // cope
                continue;
            }
        };
        defmt::info!("Response body: {}", body);

        Timer::after(Duration::from_secs(15)).await;
    }
}
