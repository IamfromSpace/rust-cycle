extern crate ansi_escapes;
extern crate bincode;
extern crate btleplug;
extern crate serde;
extern crate sled;

use ansi_escapes::CursorTo;
use btleplug::api::{BDAddr, Central, Peripheral, UUID};
use btleplug::bluez::manager::Manager;
use serde::{Deserialize, Serialize};
use std::convert::{From, TryInto};
use std::io::{stdout, Write};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// SUUID is equivalent to a UUID, however it is serializable so we can save its
// value to our sled.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub enum SUUID {
    B16(u16),
    B128([u8; 16]),
}

impl From<UUID> for SUUID {
    fn from(u: UUID) -> SUUID {
        match u {
            UUID::B16(x) => SUUID::B16(x),
            UUID::B128(x) => SUUID::B128(x),
        }
    }
}

impl From<SUUID> for UUID {
    fn from(u: SUUID) -> UUID {
        match u {
            SUUID::B16(x) => UUID::B16(x),
            SUUID::B128(x) => UUID::B128(x),
        }
    }
}

// Helper function to demonstrate consumption of a DB
fn print_db(db: &sled::Tree, key_decoder: &bincode::Config) -> () {
    for x in db.iter() {
        let (k, v) = x.unwrap();
        let z: Vec<u8> = (*k).try_into().unwrap();
        let (session_key, d, suuid): (u64, Duration, SUUID) = key_decoder.deserialize(&z).unwrap();
        println!(
            "{:?}-{:?}-{:?} = {:?}",
            UNIX_EPOCH
                .checked_add(Duration::from_secs(session_key))
                .unwrap(),
            d,
            UUID::from(suuid),
            parse_hrm(&(*v).try_into().unwrap())
        );
    }
}

pub fn main() {
    let mut key_coder = bincode::config();
    let key_coder = key_coder.big_endian();
    let db = sled::open(".rust-cycle.sled").unwrap();

    print_db(&db, &key_coder);

    // We want instant, because we want this to be monotonic. We don't want
    // clock drift/corrections to cause events to be processed out of order.
    let start = Instant::now();
    // This won't fail unless the clock is before epoch, which sounds like a
    // bigger problem
    let session_key = u64::to_be_bytes(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    );

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

    let db_hrm = db.clone();
    let key_encoder_hrm = key_coder.clone();
    hrm.on_notification(Box::new(move |n| {
        print!(
            "{}HR {:?}bpm ",
            CursorTo::AbsoluteX(0),
            parse_hrm(&n.value).bpm
        );
        stdout().flush().unwrap();
        let key = key_encoder_hrm
            .serialize(&(session_key, start.elapsed(), SUUID::from(n.uuid)))
            .unwrap();
        db_hrm.insert(key, n.value).unwrap();
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

    let db_kickr = db.clone();
    let key_encoder_kickr = key_coder.clone();
    kickr.on_notification(Box::new(move |n| {
        print!(
            "{}Power {:?}W   ",
            CursorTo::AbsoluteX(16),
            parse_cycling_power_measurement(&n.value).instantaneous_power
        );
        stdout().flush().unwrap();
        let key = key_encoder_kickr
            .serialize(&(session_key, start.elapsed(), SUUID::from(n.uuid)))
            .unwrap();
        db_kickr.insert(key, n.value).unwrap();
    }));
    */

    thread::park();
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
fn parse_hrm(data: &Vec<u8>) -> HeartRateMeasurement {
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
fn parse_csc_measurement(data: &Vec<u8>) -> CscMeasurement {
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
                last_revolution_event_time: u16::from_le_bytes([
                    data[crank_index + 2],
                    data[crank_index + 3],
                ]) as f64
                    / 1024.0,
            })
        } else {
            None
        },
    }
}

#[derive(Debug, PartialEq, Eq)]
enum AccumulatedTorqueSource {
    Wheel,
    Crank,
}

// A Struct that does not care about bit compression
#[derive(Debug, PartialEq)]
pub struct CyclingPowerMeasurement {
    instantaneous_power: i16,
    // Notably this is _truly_ a percent, not a rate
    // conversion to rate would be lossly
    pedal_power_balance_percent: Option<f32>,
    // Sum of the average torque measured per source rotation. Divide by
    // rotations to get average torque or multiply by 2pi to get total energy.
    // If you know the gearing you can translate from one source to the other.
    // Divide energy by source time to get average power.
    accumulated_torque: Option<(AccumulatedTorqueSource, f64)>,
    wheel_revolution_data: Option<RevolutionData>,
    crank_revolution_data: Option<RevolutionData>,
    // TODO: There are other fields, but they're all after these or in the flags
}

// Notably, this function always assumes a valid input
fn parse_cycling_power_measurement(data: &Vec<u8>) -> CyclingPowerMeasurement {
    let has_pedal_power_balance = data[0] & 1 == 1;
    let has_accumulated_torque = data[0] & 0b100 == 0b100;
    let has_wheel_data = data[0] & 0b10000 == 0b10000;
    let has_crank_data = data[0] & 0b100000 == 0b100000;
    let power_index = 2;
    let pedal_power_balance_index = 4;
    let accumulated_torque_index =
        pedal_power_balance_index + if has_pedal_power_balance { 1 } else { 0 };
    let wheel_data_index = accumulated_torque_index + if has_accumulated_torque { 2 } else { 0 };
    let crank_data_index = wheel_data_index + if has_wheel_data { 6 } else { 0 };

    CyclingPowerMeasurement {
        instantaneous_power: i16::from_le_bytes([data[power_index], data[power_index + 1]]),
        pedal_power_balance_percent: if has_pedal_power_balance {
            Some(data[pedal_power_balance_index] as f32 / 2.0)
        } else {
            None
        },
        accumulated_torque: if has_accumulated_torque {
            let source = if data[0] & 0b1000 == 0b1000 {
                AccumulatedTorqueSource::Crank
            } else {
                AccumulatedTorqueSource::Wheel
            };
            let torque = u16::from_le_bytes([
                data[accumulated_torque_index],
                data[accumulated_torque_index + 1],
            ]) as f64
                / 32.0;
            Some((source, torque))
        } else {
            None
        },
        // This isn't quite identical to CSC wheel data: it's /2048 instead of /1024
        wheel_revolution_data: if has_wheel_data {
            Some(RevolutionData {
                revolution_count: u32::from_le_bytes([
                    data[wheel_data_index],
                    data[wheel_data_index + 1],
                    data[wheel_data_index + 2],
                    data[wheel_data_index + 3],
                ]),
                last_revolution_event_time: (u16::from_le_bytes([
                    data[wheel_data_index + 4],
                    data[wheel_data_index + 4 + 1],
                ]) as f64)
                    / 2048.0,
            })
        } else {
            None
        },
        // This is identical to CSC crank data
        crank_revolution_data: if has_crank_data {
            Some(RevolutionData {
                revolution_count: u32::from_le_bytes([
                    data[crank_data_index],
                    data[crank_data_index + 1],
                    0,
                    0,
                ]),
                last_revolution_event_time: u16::from_le_bytes([
                    data[crank_data_index + 2],
                    data[crank_data_index + 2 + 1],
                ]) as f64
                    / 1024.0,
            })
        } else {
            None
        },
    }
}

// This is just a quick port of the original JS I had written--there's room for
// improvement
mod write_fit {
    pub struct FitRecord {
        // We use the same bitdepth, but not the same epoch
        seconds_since_unix_epoch: u32,
        // Wattage
        power: Option<u16>,
        // BPM
        heart_rate: Option<u8>,
        // RPM
        cadence: Option<u8>,
    }

    fn make_header(length: usize) -> Vec<u8> {
        vec![
            // Header length
            12,
            // protocol version
            0x20,
            // profile version (little endian)
            0xeb,
            0x07,
            // number of bytes excluding header and checksum (little endian)
            length as u8 & 0xff,
            (length >> 8) as u8 & 0xff,
            (length >> 16) as u8 & 0xff,
            (length >> 24) as u8 & 0xff,
            // ASCI for .FIT
            0x2e,
            0x46,
            0x49,
            0x54,
        ]
    }

    fn record_to_bytes(record: &FitRecord) -> Vec<u8> {
        let ts = record.seconds_since_unix_epoch - 631065600;
        let mut bytes = vec![
            0,
            // Time
            ts as u8 & 0xff,
            (ts >> 8) as u8 & 0xff,
            (ts >> 16) as u8 & 0xff,
            (ts >> 24) as u8 & 0xff,
        ];

        if let Some(p) = record.power {
            bytes.push(p as u8 & 0xff);
            bytes.push((p >> 8) as u8 & 0xff);
        };

        if let Some(hr) = record.heart_rate {
            bytes.push(hr);
        }

        if let Some(c) = record.cadence {
            bytes.push(c);
        }

        bytes
    }

    fn record_def(record: &FitRecord) -> Vec<u8> {
        let field_count = 1
            + if let Some(_) = record.power { 1 } else { 0 }
            + if let Some(_) = record.heart_rate {
                1
            } else {
                0
            }
            + if let Some(_) = record.cadence { 1 } else { 0 };

        let mut bytes = vec![
            // Field definition for message type 0
            64,
            // Reserved
            0,
            // Little Endian
            0,
            // Global Message Number (20 is for a typical data record)
            20,
            0,
            // Number of fields
            field_count,
            // Timestamp (field definition number, byte count, default type (u32))
            253,
            4,
            0x86,
        ];

        let power_def = vec![
            // Power (field definition number, byte count, default type (u16))
            7, 2, 0x84,
        ];
        let hr_def = vec![
            // HeartRate (field definition number, byte count, default type (u8))
            3, 1, 2,
        ];
        let cadence_def = vec![
            // Cadence (field definition number, byte count, default type (u8))
            4, 1, 2,
        ];

        if let Some(_) = record.power {
            bytes.extend(power_def);
        };

        if let Some(_) = record.heart_rate {
            bytes.extend(hr_def);
        }

        if let Some(_) = record.cadence {
            bytes.extend(cadence_def);
        }

        bytes
    }

    fn calculate_crc(blob: &Vec<u8>) -> u16 {
        let crc_table = [
            0x0000, 0xcc01, 0xd801, 0x1400, 0xf001, 0x3c00, 0x2800, 0xe401, 0xa001, 0x6c00, 0x7800,
            0xb401, 0x5000, 0x9c01, 0x8801, 0x4400,
        ];

        let mut crc = 0;
        for i in 0..blob.len() {
            let byte = blob[i] as u16;
            let mut tmp = crc_table[(crc & 0xf) as usize];
            crc = (crc >> 4) & 0x0fff;
            crc = crc ^ tmp ^ crc_table[(byte & 0xf) as usize];
            tmp = crc_table[(crc & 0xf) as usize];
            crc = (crc >> 4) & 0x0fff;
            crc = crc ^ tmp ^ crc_table[((byte >> 4) & 0xf) as usize];
        }

        crc
    }

    fn to_file_inner(list: &Vec<FitRecord>) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut last_def: Option<Vec<u8>> = None;

        for record in list {
            let new_def = record_def(record);
            match last_def {
                Some(ld) => {
                    if ld != new_def {
                        last_def = Some(new_def.clone());
                        bytes.extend(new_def)
                    } else {
                        last_def = Some(ld);
                    }
                }
                None => {
                    last_def = Some(new_def.clone());
                    bytes.extend(new_def);
                }
            }

            bytes.extend(record_to_bytes(record));
        }

        bytes
    }

    pub fn to_file(list: &Vec<FitRecord>) -> Vec<u8> {
        let record_buffer = to_file_inner(list);
        let mut bytes = make_header(record_buffer.len());
        bytes.extend(record_buffer);
        let crc = calculate_crc(&bytes);
        bytes.extend(vec![(crc & 0xff) as u8, ((crc >> 8) as u8) & 0xff]);
        bytes
    }

    #[cfg(test)]
    mod tests {
        use super::to_file;
        use super::FitRecord;

        #[test]
        fn to_file_for_empty_vec() {
            assert_eq!(
                vec!(
                    0x0c, 0x20, 0xeb, 0x07, 0x00, 0x00, 0x00, 0x00, 0x2e, 0x46, 0x49, 0x54, 0x36,
                    0xc1
                ),
                to_file(&Vec::new()),
            );
        }

        #[test]
        fn to_file_for_single_record() {
            assert_eq!(
                vec!(
                    0x0c, 0x20, 0xeb, 0x07, 0x1b, 0x00, 0x00, 0x00, 0x2e, 0x46, 0x49, 0x54, 0x40,
                    0x00, 0x00, 0x14, 0x00, 0x04, 0xfd, 0x04, 0x86, 0x07, 0x02, 0x84, 0x03, 0x01,
                    0x02, 0x04, 0x01, 0x02, 0x00, 0xe8, 0x98, 0xc9, 0x38, 0xb4, 0x00, 0x78, 0x5a,
                    0xe4, 0xc1
                ),
                to_file(&vec!(FitRecord {
                    seconds_since_unix_epoch: 1583801576,
                    power: Some(180),
                    heart_rate: Some(120),
                    cadence: Some(90)
                })),
            );
        }

        #[test]
        fn to_file_for_two_records() {
            assert_eq!(
                vec!(
                    0x0c, 0x20, 0xeb, 0x07, 0x24, 0x00, 0x00, 0x00, 0x2e, 0x46, 0x49, 0x54, 0x40,
                    0x00, 0x00, 0x14, 0x00, 0x04, 0xfd, 0x04, 0x86, 0x07, 0x02, 0x84, 0x03, 0x01,
                    0x02, 0x04, 0x01, 0x02, 0x00, 0xe8, 0x98, 0xc9, 0x38, 0xb4, 0x00, 0x78, 0x5a,
                    0x00, 0xe9, 0x98, 0xc9, 0x38, 0xb5, 0x00, 0x79, 0x5b, 0x7b, 0x97
                ),
                to_file(&vec!(
                    FitRecord {
                        seconds_since_unix_epoch: 1583801576,
                        power: Some(180),
                        heart_rate: Some(120),
                        cadence: Some(90)
                    },
                    FitRecord {
                        seconds_since_unix_epoch: 1583801577,
                        power: Some(181),
                        heart_rate: Some(121),
                        cadence: Some(91)
                    }
                )),
            );
        }

        #[test]
        fn to_file_for_single_record_without_power() {
            assert_eq!(
                vec!(
                    0x0c, 0x20, 0xeb, 0x07, 0x16, 0x00, 0x00, 0x00, 0x2e, 0x46, 0x49, 0x54, 0x40,
                    0x00, 0x00, 0x14, 0x00, 0x03, 0xfd, 0x04, 0x86, 0x03, 0x01, 0x02, 0x04, 0x01,
                    0x02, 0x00, 0xe8, 0x98, 0xc9, 0x38, 0x78, 0x5a, 0x9b, 0x59
                ),
                to_file(&vec!(FitRecord {
                    seconds_since_unix_epoch: 1583801576,
                    power: None,
                    heart_rate: Some(120),
                    cadence: Some(90)
                })),
            );
        }

        #[test]
        fn to_file_for_single_record_without_heart_rate() {
            assert_eq!(
                vec!(
                    0x0c, 0x20, 0xeb, 0x07, 0x17, 0x00, 0x00, 0x00, 0x2e, 0x46, 0x49, 0x54, 0x40,
                    0x00, 0x00, 0x14, 0x00, 0x03, 0xfd, 0x04, 0x86, 0x07, 0x02, 0x84, 0x04, 0x01,
                    0x02, 0x00, 0xe8, 0x98, 0xc9, 0x38, 0xb4, 0x00, 0x5a, 0xf9, 0xbe
                ),
                to_file(&vec!(FitRecord {
                    seconds_since_unix_epoch: 1583801576,
                    power: Some(180),
                    heart_rate: None,
                    cadence: Some(90)
                })),
            );
        }

        #[test]
        fn to_file_for_single_record_without_cadence() {
            assert_eq!(
                vec!(
                    0x0c, 0x20, 0xeb, 0x07, 0x17, 0x00, 0x00, 0x00, 0x2e, 0x46, 0x49, 0x54, 0x40,
                    0x00, 0x00, 0x14, 0x00, 0x03, 0xfd, 0x04, 0x86, 0x07, 0x02, 0x84, 0x03, 0x01,
                    0x02, 0x00, 0xe8, 0x98, 0xc9, 0x38, 0xb4, 0x00, 0x78, 0x63, 0xd3
                ),
                to_file(&vec!(FitRecord {
                    seconds_since_unix_epoch: 1583801576,
                    power: Some(180),
                    heart_rate: Some(120),
                    cadence: None
                })),
            );
        }
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
            parse_hrm(&vec!(0b11001, 70, 0, 11, 2, 10, 1))
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
            parse_hrm(&vec!(0b10001, 70, 0, 10, 1))
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
            parse_hrm(&vec!(0b10000, 70, 10, 1, 11, 2, 12, 3))
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
            parse_hrm(&vec!(0b10000, 70, 10, 1))
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
            parse_hrm(&vec!(0b1001, 70, 0, 10, 1))
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
            parse_hrm(&vec!(0b1000, 70, 10, 1))
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
            parse_hrm(&vec!(0b100, 70))
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
            parse_hrm(&vec!(0b110, 70))
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
            parse_hrm(&vec!(1, 10, 1))
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
            parse_hrm(&vec!(1, 70, 0))
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
            parse_hrm(&vec!(0, 70))
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
            parse_csc_measurement(&vec!(3, 1, 2, 3, 4, 1, 2, 1, 2, 1, 2))
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
            parse_csc_measurement(&vec!(2, 1, 2, 1, 2))
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
            parse_csc_measurement(&vec!(1, 1, 2, 3, 4, 1, 2))
        );
    }

    #[test]
    fn parse_csc_empty() {
        assert_eq!(
            CscMeasurement {
                wheel: None,
                crank: None,
            },
            parse_csc_measurement(&vec!(0))
        );
    }

    use super::parse_cycling_power_measurement;
    use super::AccumulatedTorqueSource;
    use super::CyclingPowerMeasurement;

    #[test]
    fn parse_cpm_with_balance_torque_wheel_and_crank() {
        assert_eq!(
            CyclingPowerMeasurement {
                instantaneous_power: 0x0102,
                pedal_power_balance_percent: Some(49.5),
                accumulated_torque: Some((AccumulatedTorqueSource::Wheel, 0x0201 as f64 / 32.0)),
                wheel_revolution_data: Some(RevolutionData {
                    revolution_count: 0x04030201,
                    last_revolution_event_time: 0x0201 as f64 / 2048.0,
                }),
                crank_revolution_data: Some(RevolutionData {
                    revolution_count: 0x0201,
                    last_revolution_event_time: 0x0201 as f64 / 1024.0,
                }),
            },
            parse_cycling_power_measurement(&vec!(
                0b110101, 0, 2, 1, 99, 1, 2, 1, 2, 3, 4, 1, 2, 1, 2, 1, 2
            ))
        );
    }

    #[test]
    fn parse_cpm_with_accumulated_crank_torque() {
        assert_eq!(
            CyclingPowerMeasurement {
                instantaneous_power: 0x0102,
                pedal_power_balance_percent: None,
                accumulated_torque: Some((AccumulatedTorqueSource::Crank, 0x0201 as f64 / 32.0)),
                wheel_revolution_data: None,
                crank_revolution_data: Some(RevolutionData {
                    revolution_count: 0x0201,
                    last_revolution_event_time: 0x0201 as f64 / 1024.0,
                }),
            },
            parse_cycling_power_measurement(&vec!(0b101100, 0, 2, 1, 1, 2, 1, 2, 1, 2))
        );
    }

    #[test]
    fn parse_cpm_with_accumulated_wheel_torque() {
        assert_eq!(
            CyclingPowerMeasurement {
                instantaneous_power: 0x0102,
                pedal_power_balance_percent: None,
                accumulated_torque: Some((AccumulatedTorqueSource::Wheel, 0x0201 as f64 / 32.0)),
                wheel_revolution_data: Some(RevolutionData {
                    revolution_count: 0x04030201,
                    last_revolution_event_time: 0x0201 as f64 / 2048.0,
                }),
                crank_revolution_data: None,
            },
            parse_cycling_power_measurement(&vec!(0b10100, 0, 2, 1, 1, 2, 1, 2, 3, 4, 1, 2))
        );
    }

    #[test]
    fn parse_cpm_with_pedal_power_balance() {
        assert_eq!(
            CyclingPowerMeasurement {
                instantaneous_power: 0x0102,
                pedal_power_balance_percent: Some(49.5),
                accumulated_torque: None,
                wheel_revolution_data: None,
                crank_revolution_data: None,
            },
            parse_cycling_power_measurement(&vec!(1, 0, 2, 1, 99))
        );
    }

    #[test]
    fn parse_cpm_empty() {
        assert_eq!(
            CyclingPowerMeasurement {
                instantaneous_power: 0x0102,
                pedal_power_balance_percent: None,
                accumulated_torque: None,
                wheel_revolution_data: None,
                crank_revolution_data: None,
            },
            parse_cycling_power_measurement(&vec!(0, 0, 2, 1))
        );
    }
}
