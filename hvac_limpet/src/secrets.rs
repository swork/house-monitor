use defmt::*;
use serde::Deserialize;
use serde_json_core::de;

#[derive(Deserialize, Format, Clone, Copy)]
pub struct Secrets<'a> {
    //    pub version: &'a str,
    pub wifi_ssid: &'a str,
    pub wifi_password: &'a str,
    pub couchdb_url: &'a str,
    //    pub couchdb_user: &'a str,
    //    pub couchdb_password: &'a str,
}

pub fn get_secrets() -> Secrets<'static> {
    // This file is symlinked to my private 404secrets repo.
    // Anything matching the struct above will work.
    const SECRETS_SIZE: usize = 229;
    const SECRETS: &[u8; SECRETS_SIZE] = include_bytes!("../../secrets.json");
    match de::from_slice::<Secrets<'_>>(SECRETS) {
        Ok((r, _)) => r,
        Err(_e) => {
            defmt::panic!("JSON secrets parse error");
        }
    }
}
