extern crate btleplug;

use std::io::{stdout,Write};
use std::thread;
use std::time::Duration;
use btleplug::bluez::manager::Manager;
use btleplug::api::{UUID, Central, Peripheral};

pub fn main() {
    println!("Getting Manager...");
    stdout().flush().unwrap();

    let manager = Manager::new().unwrap();

    let mut adapter = manager.adapters().unwrap().into_iter().next().unwrap();

    adapter = manager.down(&adapter).unwrap();
    adapter = manager.up(&adapter).unwrap();

    let central = adapter.connect().unwrap();

    println!("Starting Scan...");
    stdout().flush().unwrap();
    central.start_scan().unwrap();

    thread::sleep(Duration::from_secs(5));

    println!("Stopping scan...");
    stdout().flush().unwrap();
    central.stop_scan().unwrap();

    println!("{:?}", central.peripherals());

    let kickr = central.peripherals().into_iter()
        .find(|p| p.properties().local_name.iter()
              .any(|name| name.contains("KICKR"))).unwrap();
    println!("Found KICKR");
    stdout().flush().unwrap();

    kickr.connect().unwrap();
    println!("Connected to KICKR");
    stdout().flush().unwrap();

    kickr.discover_characteristics().unwrap();
    println!("All characteristics discovered");
    stdout().flush().unwrap();

    println!("{:?}", kickr.characteristics());
    let power_measurement = kickr.characteristics().into_iter().find(|c| c.uuid == UUID::B16(0x2A63)).unwrap();

    kickr.subscribe(&power_measurement).unwrap();
    println!("Subscribed to power measure");
    stdout().flush().unwrap();

    kickr.on_notification(Box::new(|n| {
        println!("{:?}", n);
        stdout().flush().unwrap();
    }));
    
    loop {}
}

// A Struct that does not care about bit compression
#[derive(Debug, PartialEq)]
pub struct HeartRateMeasurement {
    // since this is not in SI, its units are defined in its name.
    bpm: u16,
    // If sensor contact is not supported, this is None, otherwise the boolean
    // value will tell you.
    is_sensor_contact_detected: Option<bool>,
    // Note that this _could_ overflow for very very long rides, but that makes
    // an otherwise snapshot-only measurement need prior context.  This is in
    // Joules.
    energy_expended: Option<u16>,
    // If this is present, there is guaranteed to be at least one entry (I
    // think).  A 32-bit float is a lossless representation of the original
    // data sent by the device.
    rr_intervals: Option<Vec<f32>>,
}

// Notably, this function always assumes a valid input
fn parse_hrm(data: Vec<u8>) -> HeartRateMeasurement {
    let is_16_bit = data[0] & 1 == 1;
    let has_sensor_detection = data[0] & 0b100 == 0b100;
    let has_energy_expended = data[0] & 0b1000 == 0b1000;
    let has_rr_intervals = data[0] & 0b10000 == 0b10000;
    let energy_expended_index = 2 + if is_16_bit { 1 } else { 0 };
    let rr_interval_index =
        2 + if has_energy_expended { 2 } else { 0 } + if is_16_bit { 1 } else { 0 };
    HeartRateMeasurement {
        bpm: if is_16_bit {
            ((data[2] as u16) << 8) + (data[1] as u16)
        } else {
            data[1] as u16
        },
        is_sensor_contact_detected: if has_sensor_detection {
            Some(data[0] & 0b10 == 0b10)
        } else {
            None
        },
        energy_expended: if has_energy_expended {
            Some(
                ((data[energy_expended_index + 1] as u16) << 8)
                    + (data[energy_expended_index] as u16),
            )
        } else {
            None
        },
        rr_intervals: if has_rr_intervals {
            let rr_interval_count = (data.len() - rr_interval_index) / 2;
            let mut vec = Vec::with_capacity(rr_interval_count);
            for i in 0..rr_interval_count {
                println!("{}", rr_interval_index);
                vec.push(
                    ((((data[rr_interval_index + 2 * i + 1] as u16) << 8)
                        + (data[rr_interval_index + 2 * i] as u16)) as f32)
                        / 1024.0,
                );
            }
            Some(vec)
        } else {
            None
        },
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn my_test() {
        assert_eq!(true, true);
    }
}
