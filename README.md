# House Monitor

A house with a zoned heat pump and an obsolete security system wants monitoring, which might be the basis of some control functions. Here's some monitoring.

## hvac_limpet

A Raspberry Pi Pico W microcontroller board is put on top of a Honeywell TZ-4 zone controller with GPIO pins interfaced through simple signal conditioning to the controller's status LEDs. GPIO is watched for changes, and at each a record is logged to a database. The limpet includes some temperature sensors, values for which are included in each record. 

This dir is a Rust crate, cross-compiling with "cargo build" to ELF output target/thumb*/debug/hvac_limpet. The host requires an arm-none-eabi toolchain. This file gets written to the RPi Pico W, and that gets wired into the zone controller.

You must supply ./secrets.json, ideally a symlink to a file, containing WIFI SSID and credentials and the
CouchDB database endpoint.