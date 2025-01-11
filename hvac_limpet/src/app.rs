use crate::secrets::Secrets;
use crate::transport::DataLogTransport;

use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::select::{select, select_slice};
use embassy_net::Stack;
use embassy_rp::gpio::{Flex, Input};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::{Channel, Sender};
use embassy_time::{Duration, Timer};
// use field_count::FieldCount; -- I need it const, crate needs 
use heapless::Vec;
use static_cell::StaticCell;

// Accept GPIO pins monitoring LEDs from system into this struct.
// #[derive(FieldCount)]
pub struct InputPinsMonitoringLeds<'a> {
    pub heat: &'a mut Input<'a>,
    pub cool: &'a mut Input<'a>,
    pub emergency: &'a mut Input<'a>,
    pub purge: &'a mut Input<'a>,
    pub zone1: &'a mut Input<'a>,
    pub zone2: &'a mut Input<'a>,
    pub zone3: &'a mut Input<'a>,
    pub zone4: &'a mut Input<'a>,
}
const IN_LEDS_COUNT: usize = 8; // InputPinsMonitoringLeds::field_count();

pub struct IoPinsOneWire<'a> {
    pub onewire: &'a mut Flex<'a>,
}

enum TriggerMessage {
    Trigger,
}

type TriggerChannel = Channel<NoopRawMutex, TriggerMessage, 10>;

#[embassy_executor::task]
pub async fn monitor_inputs(
    funnel: Sender<'static, NoopRawMutex, TriggerMessage, 10>,
    mut triggers: Vec<&'static mut Input<'static>, IN_LEDS_COUNT>,
) -> ! {
    loop {
        let mut futures = Vec::<_, IN_LEDS_COUNT>::new();
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

pub async fn run<'a>(
    spawner: Spawner,
    in_pins_leds: &'static mut InputPinsMonitoringLeds<'static>,
    stack: Stack<'static>,
    secrets: Secrets<'static>,
    seed: u64,
) -> ! {
    let data_log_transport = DataLogTransport::new();
    let mut triggers = Vec::<&'static mut Input, IN_LEDS_COUNT>::new();
    let _ = triggers.push(in_pins_leds.heat);
    let _ = triggers.push(in_pins_leds.cool);
    let _ = triggers.push(in_pins_leds.purge);
    let _ = triggers.push(in_pins_leds.emergency);
    let _ = triggers.push(in_pins_leds.zone1);
    let _ = triggers.push(in_pins_leds.zone2);
    let _ = triggers.push(in_pins_leds.zone3);
    let oops = triggers.push(in_pins_leds.zone4);
    if let Err(_) = oops {
        defmt::panic!("Too many pushes to heapless::Vec (how?)");
    }

    // 10 deep, 2s between possible sends leaves 20s for blocked network send
    // before errors accumulate. Be sure queuing errors are handled robustly.
    static TRIGGER_CHANNEL: StaticCell<TriggerChannel> = StaticCell::new();
    let trigger_channel = TRIGGER_CHANNEL.init(TriggerChannel::new());

    unwrap!(spawner.spawn(monitor_inputs(trigger_channel.sender(), triggers)));

    let receiver = trigger_channel.receiver();
    data_log_transport.zip_one_off(stack, seed, secrets, /*GatheredInfo::empty()*/).await;
    loop {
        select(receiver.receive(), Timer::after(Duration::from_secs(4*3600))).await;
        data_log_transport.zip_one_off(stack, seed, secrets, /*GatheredInfo::collect()*/).await;
    }
}
