mod char_db;
mod fit;
mod peripherals;

use ansi_escapes::CursorTo;
use btleplug::api::{Central, CentralEvent, Peripheral, UUID};
use btleplug::bluez::manager::Manager;
use peripherals::kickr::Kickr;
use std::collections::BTreeSet;
use std::env;
use std::fs::File;
use std::io::{stdout, Write};
use std::mem;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// TODO: More complex workouts
const POWER_TARGET: u8 = 160;

pub fn main() {
    let args: BTreeSet<String> = env::args().collect();
    let use_hr = args.is_empty() || args.contains("--hr");
    let use_cadence = args.is_empty() || args.contains("--cadence");
    let use_power = args.is_empty() || args.contains("--power");
    let is_output_mode = args.is_empty() || args.contains("--output");
    if !use_hr && !use_cadence && !use_power && !is_output_mode {
        panic!("No metrics selected!");
    }

    let db = char_db::open_default().unwrap();

    if is_output_mode {
        // TODO: Should accept a cli flag for output mode vs session mode
        let most_recent_session = db.get_most_recent_session().unwrap().unwrap();
        File::create("workout.fit")
            .unwrap()
            .write_all(&db_session_to_fit(&db, most_recent_session)[..])
            .unwrap();
    } else {
        // We want instant, because we want this to be monotonic. We don't want
        // clock drift/corrections to cause events to be processed out of order.
        let start = Instant::now();
        // This won't fail unless the clock is before epoch, which sounds like a
        // bigger problem
        let session_key = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        println!("Getting Manager...");
        let manager = Manager::new().unwrap();

        let mut adapter = manager.adapters().unwrap().into_iter().next().unwrap();

        adapter = manager.down(&adapter).unwrap();
        adapter = manager.up(&adapter).unwrap();

        let central = adapter.connect().unwrap();
        // There's a bug in 0.4 that does not default the scan to active.
        // Without an active scan the Polar H10 will not give back its name.
        // TODO: remove this line after merge and upgrade.
        central.active(true);

        println!("Starting Scan...");
        central.start_scan().unwrap();

        thread::sleep(Duration::from_secs(5));

        println!("Stopping scan...");
        central.stop_scan().unwrap();

        if use_hr {
            // Connect to HRM and print its parsed notifications
            let hrm = central
                .peripherals()
                .into_iter()
                .find(|p| {
                    p.properties()
                        .local_name
                        .iter()
                        .any(|name| name.contains("Polar"))
                })
                .unwrap();
            println!("Found HRM");

            hrm.connect().unwrap();
            println!("Connected to HRM");

            hrm.discover_characteristics().unwrap();
            println!("All characteristics discovered");

            let hr_measurement = hrm
                .characteristics()
                .into_iter()
                .find(|c| c.uuid == UUID::B16(0x2A37))
                .unwrap();

            hrm.subscribe(&hr_measurement).unwrap();
            println!("Subscribed to hr measure");

            let db_hrm = db.clone();
            let mut i = 0;
            hrm.on_notification(Box::new(move |n| {
                let elapsed = start.elapsed();
                i += 1;
                print!(
                    "{}{}:{:02}   {}HR {:?}bpm ({}) ",
                    CursorTo::AbsoluteX(0),
                    elapsed.as_secs() / 60,
                    elapsed.as_secs() % 60,
                    CursorTo::AbsoluteX(9),
                    parse_hrm(&n.value).bpm,
                    i
                );
                stdout().flush().unwrap();
                db_hrm.insert(session_key, elapsed, n).unwrap();
            }));
        }

        if use_power {
            // Connect to Kickr and print its raw notifications
            let kickr = Kickr::new(central.clone()).unwrap();

            let db_kickr = db.clone();
            let mut i = 0;
            kickr.on_notification(Box::new(move |n| {
                if n.uuid == UUID::B16(0x2A63) {
                    let elapsed = start.elapsed();
                    i += 1;
                    print!(
                        "{}{}:{:02}   {}Power {:?}W ({})  ",
                        CursorTo::AbsoluteX(0),
                        elapsed.as_secs() / 60,
                        elapsed.as_secs() % 60,
                        CursorTo::AbsoluteX(32),
                        parse_cycling_power_measurement(&n.value).instantaneous_power,
                        i
                    );
                    stdout().flush().unwrap();
                    db_kickr.insert(session_key, elapsed, n).unwrap();
                } else {
                    println!("Non-power notification from kickr: {:?}", n);
                }
            }));

            kickr.set_power(POWER_TARGET).unwrap();
        }

        if use_cadence {
            // Connect to Cadence meter and print its raw notifications
            let cadence_measure = central
                .peripherals()
                .into_iter()
                .find(|p| {
                    p.properties()
                        .local_name
                        .iter()
                        .any(|name| name.contains("CADENCE"))
                })
                .unwrap();

            println!("Found CADENCE");

            cadence_measure.connect().unwrap();
            println!("Connected to CADENCE");

            cadence_measure.discover_characteristics().unwrap();
            println!("All characteristics discovered");

            let cadence_measurement = cadence_measure
                .characteristics()
                .into_iter()
                .find(|c| c.uuid == UUID::B16(0x2A5B))
                .unwrap();

            cadence_measure.subscribe(&cadence_measurement).unwrap();
            println!("Subscribed to cadence measure");

            let mut o_last_cadence_measure: Option<CscMeasurement> = None;
            let db_cadence_measure = db.clone();
            let mut i = 0;
            cadence_measure.on_notification(Box::new(move |n| {
                let elapsed = start.elapsed();
                let csc_measure = parse_csc_measurement(&n.value);
                let last_cadence_measure = mem::replace(&mut o_last_cadence_measure, None);
                if let Some(last_cadence_measure) = last_cadence_measure {
                    let a = last_cadence_measure.crank.unwrap();
                    let b = csc_measure.crank.as_ref().unwrap();
                    i += 1;
                    if let Some(rpm) = overflow_protected_rpm(&a, &b) {
                        print!(
                            "{}{}:{:02}   {}Cadence {:?}rpm ({}) ",
                            CursorTo::AbsoluteX(0),
                            elapsed.as_secs() / 60,
                            elapsed.as_secs() % 60,
                            CursorTo::AbsoluteX(55),
                            rpm as u8,
                            i
                        );
                        stdout().flush().unwrap();
                    }
                }
                o_last_cadence_measure = Some(csc_measure);
                db_cadence_measure.insert(session_key, elapsed, n).unwrap();
            }));
        }

        let central_for_disconnects = central.clone();
        central.on_event(Box::new(move |evt| {
            println!("{:?}", evt);
            match evt {
                CentralEvent::DeviceDisconnected(addr) => {
                    println!("PERIPHERAL DISCONNECTED");
                    let p = central_for_disconnects.peripheral(addr).unwrap();
                    // Kickr is handled on its own
                    if !peripherals::kickr::is_kickr(&p) {
                        thread::sleep(Duration::from_secs(2));
                        p.reconnect().unwrap();

                        println!("PERIPHERAL RECONNECTED");
                    }
                }
                _ => {}
            }
        }));

        thread::park();
    }
}

// A Struct that does not care about bit compression
#[derive(Debug, PartialEq, Clone)]
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
#[derive(Debug, PartialEq, Clone)]
pub struct RevolutionData {
    // Total number of revolutions, this is years of data for wheels and cranks
    revolution_count: u32,
    // The time (in seconds) that the last revolution finished, this type is
    // chosen because it is both lossless and holds years of data.
    last_revolution_event_time: f64,
}

// A Struct that does not care about bit compression
#[derive(Debug, PartialEq, Clone)]
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

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum AccumulatedTorqueSource {
    Wheel,
    Crank,
}

// A Struct that does not care about bit compression
#[derive(Debug, PartialEq, Clone)]
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

fn overflow_protected_rpm(a: &RevolutionData, b: &RevolutionData) -> Option<f64> {
    if a.last_revolution_event_time == b.last_revolution_event_time {
        None
    } else {
        let duration = if b.last_revolution_event_time > a.last_revolution_event_time {
            b.last_revolution_event_time - a.last_revolution_event_time
        } else {
            0b100000 as f64 + b.last_revolution_event_time - a.last_revolution_event_time
        };

        let total_revolutions = if b.revolution_count > a.revolution_count {
            b.revolution_count - a.revolution_count
        } else {
            0b100000 + b.revolution_count - a.revolution_count
        };

        Some(total_revolutions as f64 * 60.0 / duration)
    }
}

fn db_session_to_fit(db: &char_db::CharDb, session_key: u64) -> Vec<u8> {
    let mut last_power: u16 = 0;
    let mut last_csc_measurement: Option<CscMeasurement> = None;
    let mut record: Option<fit::FitRecord> = None;
    let mut records = Vec::new();
    let empty_record = |t| fit::FitRecord {
        seconds_since_unix_epoch: t,
        power: None,
        heart_rate: None,
        cadence: None,
    };

    for x in db.get_session_entries(session_key) {
        if let Ok(((_, d, uuid), v)) = x {
            let seconds_since_unix_epoch = (session_key + d.as_secs()) as u32;
            let mut r = match record {
                Some(mut r) => {
                    if r.seconds_since_unix_epoch == seconds_since_unix_epoch {
                        r
                    } else {
                        if let None = r.power {
                            r.power = Some(last_power);
                        }
                        records.push(r);
                        empty_record(seconds_since_unix_epoch)
                    }
                }
                None => empty_record(seconds_since_unix_epoch),
            };

            record = Some(match uuid {
                UUID::B16(0x2A37) => {
                    r.heart_rate = Some(parse_hrm(&v).bpm as u8);
                    r
                }
                UUID::B16(0x2A63) => {
                    let p = parse_cycling_power_measurement(&v).instantaneous_power as u16;
                    last_power = p;
                    r.power = Some(p);
                    r
                }
                UUID::B16(0x2A5B) => {
                    let csc_measurement = parse_csc_measurement(&v);
                    if let Some(lcm) = last_csc_measurement {
                        let a = lcm.crank.unwrap();
                        let b = csc_measurement.crank.clone().unwrap();
                        if let Some(rpm) = overflow_protected_rpm(&a, &b) {
                            r.cadence = Some(rpm as u8);
                        }
                    }
                    last_csc_measurement = Some(csc_measurement);
                    r
                }
                _ => {
                    println!("UUID not matched");
                    r
                }
            });
        }
    }

    fit::to_file(&records)
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
