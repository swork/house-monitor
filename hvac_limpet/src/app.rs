use cyw43::Control;
use crate::secrets::Secrets;

use core::str;
use defmt::*;
use embassy_net::Stack;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_time::{Duration, Timer};
use reqwless::client::{HttpClient, TlsConfig, TlsVerify};
use reqwless::request::Method;
use static_cell::ConstStaticCell;


pub async fn run(_control: Control<'_>, stack: Stack<'_>, secrets: Secrets<'_>, seed: u64) -> () {
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