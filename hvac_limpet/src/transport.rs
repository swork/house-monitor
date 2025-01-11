use core::str;
use defmt::*;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_net::Stack;
use reqwless::client::{HttpClient, TlsConfig, TlsVerify};
use reqwless::request::Method;
use static_cell::{ConstStaticCell, StaticCell};
use crate::secrets::Secrets;

pub struct DataLogTransport {
    rx_buffer: &'static mut [u8; 4192],
    tls_read_buffer: &'static mut [u8; 16640],
    tls_write_buffer: &'static mut [u8; 16640],
}

impl DataLogTransport {
    pub fn new() -> &'static mut DataLogTransport {
        DATA_LOG_TRANSPORT.init(
            Self {
                rx_buffer: RX_BUFFER.take(),
                tls_read_buffer: TLS_READ_BUFFER.take(),
                tls_write_buffer: TLS_WRITE_BUFFER.take(),
            }
        )
    }

    pub async fn zip_one_off(&mut self, stack: Stack<'_>, seed: u64, secrets: Secrets<'_>) -> () {
        let url = secrets.couchdb_url;
        let client_state = TcpClientState::<1, 1024, 1024>::new();
        let tcp_client = TcpClient::new(stack, &client_state);
        let dns_client = DnsSocket::new(stack);
        let tls_config =
            TlsConfig::new(seed, self.tls_read_buffer, self.tls_write_buffer, TlsVerify::None);
        let mut http_client = HttpClient::new_with_tls(&tcp_client, &dns_client, tls_config);
        // or HttpClient::new(&tcp_client, &dns_client)

        info!("connecting to {}", &url);

        let mut request = match http_client.request(Method::GET, &url).await {
            Ok(req) => req,
            Err(e) => {
                error!("Failed to make HTTP request: {}", e);
                // cope
                return ();
            }
        };

        let response = match request.send(self.rx_buffer).await {
            Ok(resp) => resp,
            Err(_e) => {
                error!("Failed to send HTTP request");
                // cope
                return ();
            }
        };

        let body = match str::from_utf8(response.body().read_to_end().await.unwrap()) {
            Ok(b) => b,
            Err(_e) => {
                error!("Failed to read response body");
                // cope
                return ();
            }
        };
        defmt::info!("Response body: {}", body);
        ()
    }
}

static DATA_LOG_TRANSPORT: StaticCell<DataLogTransport> = StaticCell::new();
static RX_BUFFER: ConstStaticCell<[u8; 4192]> = ConstStaticCell::new([0; 4192]);
static TLS_READ_BUFFER: ConstStaticCell<[u8; 16640]> = ConstStaticCell::new([0; 16640]);
static TLS_WRITE_BUFFER: ConstStaticCell<[u8; 16640]> = ConstStaticCell::new([0; 16640]);
