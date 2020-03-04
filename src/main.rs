extern crate btleplug;

use btleplug::api::{BDAddr, Central, Peripheral, UUID};
use btleplug::bluez::manager::Manager;
use std::io::{stdout, Write};
use std::thread;
use std::time::Duration;

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

    // Connect to HRM and print its parsed notifications
    let hrm = central
        .peripheral(BDAddr {
            address: [0xA0, 0x26, 0xBD, 0xF7, 0xB2, 0xED],
        })
        .unwrap();
    println!("Found HRM");
    stdout().flush().unwrap();

    hrm.connect().unwrap();
    println!("Connected to HRM");
    stdout().flush().unwrap();

    hrm.discover_characteristics().unwrap();
    println!("All characteristics discovered");
    stdout().flush().unwrap();

    println!("{:?}", hrm.characteristics());
    let hr_measurement = hrm
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == UUID::B16(0x2A37))
        .unwrap();

    hrm.subscribe(&hr_measurement).unwrap();
    println!("Subscribed to hr measure");
    stdout().flush().unwrap();

    hrm.on_notification(Box::new(|n| {
        println!("{:?}", parse_hrm(n.value));
        stdout().flush().unwrap();
    }));

    /*
    // Connect to Kickr and print its raw notifications
    let kickr = central
        .peripherals()
        .into_iter()
        .find(|p| {
            p.properties()
                .local_name
                .iter()
                .any(|name| name.contains("KICKR"))
        })
        .unwrap();

    println!("Found KICKR");
    stdout().flush().unwrap();

    kickr.connect().unwrap();
    println!("Connected to KICKR");
    stdout().flush().unwrap();

    kickr.discover_characteristics().unwrap();
    println!("All characteristics discovered");
    stdout().flush().unwrap();

    println!("{:?}", kickr.characteristics());
    let power_measurement = kickr
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == UUID::B16(0x2A63))
        .unwrap();

    kickr.subscribe(&power_measurement).unwrap();
    println!("Subscribed to power measure");
    stdout().flush().unwrap();

    kickr.on_notification(Box::new(|n| {
        println!("{:?}", n);
        stdout().flush().unwrap();
    }));
    */

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
    // This is list of the time (in seconds) measured between R-Wave detections.
    // It is an array, because there may be many intervals recorded during a
    // single notification window (or there may be none).  Measurements are
    // indexed by time, so the 0-index reading is the oldest. A 32-bit float is
    // a lossless representation of the original data sent by the device.  Note
    // that (at least on Polar H10 devices) when the frequency of beats is lower
    // than the frequency of notifications, there's no way to distinguish
    // between zero detections and this feature not being supported on the
    // device, which is why this is not an Option.
    rr_intervals: Vec<f32>,
}

// Notably, this function always assumes a valid input
fn parse_hrm(data: Vec<u8>) -> HeartRateMeasurement {
    let is_16_bit = data[0] & 1 == 1;
    let has_sensor_detection = data[0] & 0b100 == 0b100;
    let has_energy_expended = data[0] & 0b1000 == 0b1000;
    let energy_expended_index = 2 + if is_16_bit { 1 } else { 0 };
    let rr_interval_index =
        2 + if has_energy_expended { 2 } else { 0 } + if is_16_bit { 1 } else { 0 };
    HeartRateMeasurement {
        bpm: if is_16_bit {
            u16::from_le_bytes([data[1], data[2]])
        } else {
            data[1] as u16
        },
        is_sensor_contact_detected: if has_sensor_detection {
            Some(data[0] & 0b10 == 0b10)
        } else {
            None
        },
        energy_expended: if has_energy_expended {
            Some(u16::from_le_bytes([
                data[energy_expended_index],
                data[energy_expended_index + 1],
            ]))
        } else {
            None
        },
        rr_intervals: {
            let rr_interval_count = (data.len() - rr_interval_index) / 2;
            let mut vec = Vec::with_capacity(rr_interval_count);
            for i in 0..rr_interval_count {
                let as_u16 = u16::from_le_bytes([
                    data[rr_interval_index + 2 * i],
                    data[rr_interval_index + 2 * i + 1],
                ]);
                vec.push(as_u16 as f32 / 1024.0);
            }
            vec
        },
    }
}

// A Struct that does not care about bit compression
#[derive(Debug, PartialEq)]
pub struct RevolutionData {
    // Total number of revolutions, this is years of data for wheels and cranks
    revolution_count: u32,
    // The time (in seconds) that the last revolution finished, this type is
    // chosen because it is both lossless and holds years of data.
    last_revolution_event_time: f64,
}

// A Struct that does not care about bit compression
#[derive(Debug, PartialEq)]
pub struct CscMeasurement {
    // Data about wheel rotations
    wheel: Option<RevolutionData>,
    // Data about crank rotations
    crank: Option<RevolutionData>,
}

// Notably, this function always assumes a valid input
fn parse_csc_measurement(data: Vec<u8>) -> CscMeasurement {
    let has_wheel_data = data[0] & 1 == 1;
    let has_crank_data = data[0] & 0b10 == 0b10;
    let wheel_index = 1;
    let crank_index = wheel_index + if has_wheel_data { 6 } else { 0 };

    CscMeasurement {
        wheel: if has_wheel_data {
            Some(RevolutionData {
                revolution_count: u32::from_le_bytes([
                    data[wheel_index],
                    data[wheel_index + 1],
                    data[wheel_index + 2],
                    data[wheel_index + 3],
                ]),
                last_revolution_event_time: (u16::from_le_bytes([
                    data[wheel_index + 4],
                    data[wheel_index + 4 + 1],
                ]) as f64)
                    / 1024.0,
            })
        } else {
            None
        },
        crank: if has_crank_data {
            Some(RevolutionData {
                revolution_count: u32::from_le_bytes([
                    data[crank_index],
                    data[crank_index + 1],
                    0,
                    0,
                ]),
                last_revolution_event_time: u16::from_le_bytes([data[crank_index + 2], data[crank_index + 3]])
                    as f64
                    / 1024.0,
            })
        } else {
            None
        },
    }
}

#[cfg(test)]
mod tests {
    use super::parse_hrm;
    use super::HeartRateMeasurement;

    #[test]
    fn parse_hrm_16_bit_energy_expended_and_one_rr_intervals() {
        assert_eq!(
            HeartRateMeasurement {
                bpm: 70,
                is_sensor_contact_detected: None,
                energy_expended: Some(523),
                rr_intervals: vec!(266.0 / 1024.0)
            },
            parse_hrm(vec!(0b11001, 70, 0, 11, 2, 10, 1))
        );
    }

    #[test]
    fn parse_hrm_16_bit_and_one_rr_intervals() {
        assert_eq!(
            HeartRateMeasurement {
                bpm: 70,
                is_sensor_contact_detected: None,
                energy_expended: None,
                rr_intervals: vec!(266.0 / 1024.0)
            },
            parse_hrm(vec!(0b10001, 70, 0, 10, 1))
        );
    }

    #[test]
    fn parse_hrm_and_three_rr_intervals() {
        assert_eq!(
            HeartRateMeasurement {
                bpm: 70,
                is_sensor_contact_detected: None,
                energy_expended: None,
                rr_intervals: vec!(266.0 / 1024.0, 523.0 / 1024.0, 780.0 / 1024.0)
            },
            parse_hrm(vec!(0b10000, 70, 10, 1, 11, 2, 12, 3))
        );
    }

    #[test]
    fn parse_hrm_and_one_rr_intervals() {
        assert_eq!(
            HeartRateMeasurement {
                bpm: 70,
                is_sensor_contact_detected: None,
                energy_expended: None,
                rr_intervals: vec!(266.0 / 1024.0)
            },
            parse_hrm(vec!(0b10000, 70, 10, 1))
        );
    }

    #[test]
    fn parse_hrm_16_bit_and_energy_expended() {
        assert_eq!(
            HeartRateMeasurement {
                bpm: 70,
                is_sensor_contact_detected: None,
                energy_expended: Some(266),
                rr_intervals: Vec::with_capacity(0),
            },
            parse_hrm(vec!(0b1001, 70, 0, 10, 1))
        );
    }

    #[test]
    fn parse_hrm_and_energy_expended() {
        assert_eq!(
            HeartRateMeasurement {
                bpm: 70,
                is_sensor_contact_detected: None,
                energy_expended: Some(266),
                rr_intervals: Vec::with_capacity(0),
            },
            parse_hrm(vec!(0b1000, 70, 10, 1))
        );
    }

    #[test]
    fn parse_hrm_without_contact() {
        assert_eq!(
            HeartRateMeasurement {
                bpm: 70,
                is_sensor_contact_detected: Some(false),
                energy_expended: None,
                rr_intervals: Vec::with_capacity(0),
            },
            parse_hrm(vec!(0b100, 70))
        );
    }

    #[test]
    fn parse_hrm_with_contact() {
        assert_eq!(
            HeartRateMeasurement {
                bpm: 70,
                is_sensor_contact_detected: Some(true),
                energy_expended: None,
                rr_intervals: Vec::with_capacity(0),
            },
            parse_hrm(vec!(0b110, 70))
        );
    }

    #[test]
    fn parse_hrm_16_bit_big_simple() {
        assert_eq!(
            HeartRateMeasurement {
                bpm: 266,
                is_sensor_contact_detected: None,
                energy_expended: None,
                rr_intervals: Vec::with_capacity(0),
            },
            parse_hrm(vec!(1, 10, 1))
        );
    }

    #[test]
    fn parse_hrm_16_bit_simple() {
        assert_eq!(
            HeartRateMeasurement {
                bpm: 70,
                is_sensor_contact_detected: None,
                energy_expended: None,
                rr_intervals: Vec::with_capacity(0),
            },
            parse_hrm(vec!(1, 70, 0))
        );
    }

    #[test]
    fn parse_hrm_simplest() {
        assert_eq!(
            HeartRateMeasurement {
                bpm: 70,
                is_sensor_contact_detected: None,
                energy_expended: None,
                rr_intervals: Vec::with_capacity(0),
            },
            parse_hrm(vec!(0, 70))
        );
    }

    use super::parse_csc_measurement;
    use super::CscMeasurement;
    use super::RevolutionData;

    #[test]
    fn parse_csc_with_wheel_and_crank() {
        assert_eq!(
            CscMeasurement {
                wheel: Some(RevolutionData {
                    revolution_count: 0x04030201,
                    last_revolution_event_time: 0x0201 as f64 / 1024.0,
                }),
                crank: Some(RevolutionData {
                    revolution_count: 0x0201,
                    last_revolution_event_time: 0x0201 as f64 / 1024.0,
                }),
            },
            parse_csc_measurement(vec!(3, 1, 2, 3, 4, 1, 2, 1, 2, 1, 2))
        );
    }

    #[test]
    fn parse_csc_with_crank() {
        assert_eq!(
            CscMeasurement {
                wheel: None,
                crank: Some(RevolutionData {
                    revolution_count: 0x0201,
                    last_revolution_event_time: 0x0201 as f64 / 1024.0,
                }),
            },
            parse_csc_measurement(vec!(2, 1, 2, 1, 2))
        );
    }

    #[test]
    fn parse_csc_with_wheel() {
        assert_eq!(
            CscMeasurement {
                wheel: Some(RevolutionData {
                    revolution_count: 0x04030201,
                    last_revolution_event_time: 0x0201 as f64 / 1024.0,
                }),
                crank: None,
            },
            parse_csc_measurement(vec!(1, 1, 2, 3, 4, 1, 2))
        );
    }

    #[test]
    fn parse_csc_empty() {
        assert_eq!(
            CscMeasurement {
                wheel: None,
                crank: None,
            },
            parse_csc_measurement(vec!(0))
        );
    }
}
