mod ble;
mod buttons;
mod char_db;
mod cycle_tree;
mod display;
mod fit;
mod inky_phat;
mod peripherals;
mod workout;

use ble::{
    csc_measurement::{checked_rpm_and_new_count, parse_csc_measurement, CscMeasurement},
    cycling_power_measurement::{parse_cycling_power_measurement, CyclingPowerMeasurement},
    heart_rate_measurement::parse_hrm,
};
use btleplug::api::{Central, CentralEvent, Peripheral, UUID};
use btleplug::bluez::manager::Manager;
use peripherals::{hrm::Hrm, kickr::Kickr};
use std::collections::BTreeSet;
use std::env;
use std::fs::File;
use std::io::{stdout, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use workout::{create_big_start_interval, ramp_test, run_workout, single_value};

pub fn main() {
    env_logger::init();

    let args: BTreeSet<String> = env::args().collect();
    let is_output_mode = args.is_empty() || args.contains("--output");

    let db = char_db::open_default().unwrap();

    if is_output_mode {
        // TODO: Should accept a cli flag for output mode vs session mode
        let most_recent_session = db.get_most_recent_session().unwrap().unwrap();
        File::create("workout.fit")
            .unwrap()
            .write_all(&db_session_to_fit(&db, most_recent_session)[..])
            .unwrap();
    } else {
        // Create Our Display
        let mut display = display::Display::new(Instant::now());

        // Create our Buttons
        let mut buttons = buttons::Buttons::new();

        let profile = selection(&mut display, &mut buttons, &vec!["Zenia", "Nathan"]);

        // TODO: Select Enums
        let workout_name = match profile.as_str() {
            "Zenia" => selection(&mut display, &mut buttons, &vec!["100W"]),
            "Nathan" => selection(
                &mut display,
                &mut buttons,
                &vec!["Fixed", "Ramp", "1st Big Interval"],
            ),
            _ => panic!("Unexpected profile!"),
        };

        let workout_name = match workout_name.as_str() {
            "Fixed" => selection(
                &mut display,
                &mut buttons,
                &vec!["170W", "175W", "180W", "185W"],
            ),
            _ => workout_name,
        };

        let (use_hr, use_power, use_cadence, workout) = match workout_name.as_str() {
            "100W" => (false, true, false, single_value(100)),
            "170W" => (true, true, true, single_value(170)),
            "175W" => (true, true, true, single_value(175)),
            "180W" => (true, true, true, single_value(180)),
            "185W" => (true, true, true, single_value(185)),
            "Ramp" => (true, true, true, ramp_test(120)),
            "1st Big Interval" => (
                true,
                true,
                true,
                create_big_start_interval(
                    (Duration::from_secs(300), 140),
                    14,
                    Duration::from_secs(150),
                    (Duration::from_secs(60), 320),
                    (Duration::from_secs(90), 120),
                    Some(160),
                ),
            ),
            _ => panic!("Unexpected workout_name!"),
        };

        // We want instant, because we want this to be monotonic. We don't want
        // clock drift/corrections to cause events to be processed out of order.
        let start = Instant::now();

        // Create Our Display
        let display_mutex = Arc::new(Mutex::new(display));

        // This won't fail unless the clock is before epoch, which sounds like a
        // bigger problem
        let session_key = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        lock_and_show(
            &display_mutex,
            &format!("Welcome, {}, running {}", profile, workout_name),
        );

        lock_and_show(&display_mutex, &"Setting up Bluetooth");
        let central = or_crash_with_msg(
            &display_mutex,
            setup_ble_and_discover_devices()
                // Result to Option
                // TODO: Loses original error
                .ok()
                //aka flatten: Option<Option<T>> -> Option<T>
                .and_then(|x| x),
            "Couldn't setup bluetooth!",
        );
        lock_and_show(&display_mutex, &"Connecting to Devices.");

        if use_hr {
            // Connect to HRM and print its parsed notifications
            let hrm = or_crash_with_msg(
                &display_mutex,
                Hrm::new(central.clone()).ok().and_then(|x| x),
                "Could not connect to heart rate monitor!",
            );

            let db_hrm = db.clone();
            let display_mutex_hrm = display_mutex.clone();
            hrm.on_notification(Box::new(move |n| {
                let mut display = display_mutex_hrm.lock().unwrap();
                display.update_heart_rate(Some(parse_hrm(&n.value).bpm as u8));
                let elapsed = start.elapsed();
                db_hrm.insert(session_key, elapsed, n).unwrap();
            }));
            lock_and_show(&display_mutex, &"Setup Complete for Heart Rate Monitor");
        }

        if use_power {
            // Connect to Kickr and print its raw notifications
            let kickr = or_crash_with_msg(
                &display_mutex,
                Kickr::new(central.clone()).ok().and_then(|x| x),
                "Could not connect to kickr!",
            );

            let db_kickr = db.clone();
            let display_mutex_kickr = display_mutex.clone();
            let mut o_last_power_reading: Option<CyclingPowerMeasurement> = None;
            let mut acc_torque = 0.0;
            kickr.on_notification(Box::new(move |n| {
                if n.uuid == UUID::B16(0x2A63) {
                    let mut display = display_mutex_kickr.lock().unwrap();
                    let power_reading = parse_cycling_power_measurement(&n.value);
                    let o_a = o_last_power_reading
                        .as_ref()
                        .and_then(|x| x.accumulated_torque.map(|y| y.1));
                    let o_b = power_reading.accumulated_torque.map(|x| x.1);
                    if let (Some(a), Some(b)) = (o_a, o_b) {
                        acc_torque = acc_torque + b - a + if a > b { 2048.0 } else { 0.0 };
                        display.update_external_energy(2.0 * std::f64::consts::PI * acc_torque);
                    }
                    display.update_power(Some(power_reading.instantaneous_power));
                    o_last_power_reading = Some(power_reading);
                    let elapsed = start.elapsed();
                    db_kickr.insert(session_key, elapsed, n).unwrap();
                } else {
                    println!("Non-power notification from kickr: {:?}", n);
                }
            }));

            // run our workout
            thread::spawn(move || loop {
                run_workout(Instant::now(), &workout, |p| {
                    kickr.set_power(p).unwrap();
                })
            });

            lock_and_show(&display_mutex, &"Setup Complete for Kickr");
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
            let mut crank_count = 0;
            let db_cadence_measure = db.clone();
            let display_mutex_cadence = display_mutex.clone();
            cadence_measure.on_notification(Box::new(move |n| {
                let elapsed = start.elapsed();
                let csc_measure = parse_csc_measurement(&n.value);
                let a = o_last_cadence_measure
                    .as_ref()
                    .and_then(|x| x.crank.as_ref());
                let b = csc_measure.crank.as_ref();
                if let Some((rpm, new_crank_count)) =
                    lift_a2_option(a, b, checked_rpm_and_new_count).and_then(|x| x)
                {
                    crank_count = crank_count + new_crank_count;
                    let mut display = display_mutex_cadence.lock().unwrap();
                    display.update_cadence(Some(rpm as u8));
                    display.update_crank_count(crank_count);
                    stdout().flush().unwrap();
                }
                o_last_cadence_measure = Some(csc_measure);
                db_cadence_measure.insert(session_key, elapsed, n).unwrap();
            }));
            lock_and_show(&display_mutex, &"Setup Complete for Cadence Monitor");
        }

        let central_for_disconnects = central.clone();
        central.on_event(Box::new(move |evt| {
            println!("{:?}", evt);
            match evt {
                CentralEvent::DeviceDisconnected(addr) => {
                    println!("PERIPHERAL DISCONNECTED");
                    let p = central_for_disconnects.peripheral(addr).unwrap();
                    // Kickr/Hrm are handled on their own
                    if !peripherals::kickr::is_kickr(&p) && !peripherals::hrm::is_hrm(&p) {
                        thread::sleep(Duration::from_secs(2));
                        p.connect().unwrap();

                        println!("PERIPHERAL RECONNECTED");
                    }
                }
                _ => {}
            }
        }));

        let m_will_shutdown = Arc::new(Mutex::new(false));
        let m_will_shutdown_for_button = m_will_shutdown.clone();
        buttons.on_hold(
            buttons::Button::ButtonA,
            Duration::from_secs(5),
            Box::new(move || {
                let mut will_shutdown = m_will_shutdown_for_button.lock().unwrap();
                *will_shutdown = true;
            }),
        );

        // Update it every second
        let display_mutex_for_render = display_mutex.clone();
        let m_will_shutdown_for_render = m_will_shutdown.clone();
        let render_handle = thread::spawn(move || loop {
            {
                if *m_will_shutdown_for_render.lock().unwrap() {
                    break;
                }
            };
            let mut display = display_mutex_for_render.lock().unwrap();
            display.render();
        });

        render_handle.join().unwrap();
        lock_and_show(&display_mutex, &"Goodbye");
        thread::sleep(Duration::from_secs(5));

        // TODO: This only works _during_ a workout
        println!("Powering off");
        std::process::Command::new("sudo")
            .arg("shutdown")
            .arg("now")
            .output()
            .unwrap();
    }
}

fn lift_a2_option<A, B, C, F: Fn(A, B) -> C>(a: Option<A>, b: Option<B>, f: F) -> Option<C> {
    match (a, b) {
        (Some(a), Some(b)) => Some(f(a, b)),
        _ => None,
    }
}

fn selection(
    display: &mut display::Display,
    buttons: &mut buttons::Buttons,
    x: &Vec<&str>,
) -> String {
    if x.len() < 1 || x.len() > 4 {
        panic!("Unsupported selection length!");
    }

    let choice = Arc::new(Mutex::new(None));
    use buttons::Button;
    let bs = vec![
        Button::ButtonB,
        Button::ButtonC,
        Button::ButtonD,
        Button::ButtonE,
    ];

    for i in 0..x.len() {
        let choice_button = choice.clone();
        let x_str = x.get(i).map(|x| x.to_string()).unwrap();
        buttons.on_press(
            bs[i],
            Box::new(move || {
                let mut choice = choice_button.lock().unwrap();
                if let None = *choice {
                    *choice = Some(x_str.clone());
                }
            }),
        );
    }

    display.render_options(&x);

    let result = loop {
        let or = choice.lock().unwrap();
        if let Some(r) = or.as_ref() {
            break r.clone();
        }
        thread::sleep(Duration::from_millis(15));
    };

    for b in bs {
        buttons.clear_handlers(b);
    }

    result
}

// Creates a manager, adapter, and connects it to create a central.  That
// central preforms a 5s scan, and then that central is returned.  This returns
// a Error if there was a BLE error, and it returns an Ok(None) if there are no
// adapters available.
fn setup_ble_and_discover_devices(
) -> btleplug::Result<Option<btleplug::bluez::adapter::ConnectedAdapter>> {
    println!("Getting Manager...");
    let manager = Manager::new()?;

    let adapters = manager.adapters()?;

    match adapters.into_iter().next() {
        Some(adapter) => {
            manager.down(&adapter)?;
            manager.up(&adapter)?;

            let central = adapter.connect()?;
            // There's a bug in 0.4 that does not default the scan to active.
            // Without an active scan the Polar H10 will not give back its name.
            // TODO: remove this line after merge and upgrade.
            central.active(true);

            println!("Starting Scan...");
            central.start_scan()?;

            thread::sleep(Duration::from_secs(5));

            println!("Stopping scan...");
            central.stop_scan()?;
            Ok(Some(central))
        }
        None => Ok(None),
    }
}

fn lock_and_show(display_mutex: &Arc<Mutex<display::Display>>, msg: &str) {
    let mut display = display_mutex.lock().unwrap();
    display.render_msg(msg);
}

fn or_crash_with_msg<T>(
    display_mutex: &Arc<Mutex<display::Display>>,
    x: Option<T>,
    msg: &'static str,
) -> T {
    match x {
        Some(y) => y,
        None => {
            lock_and_show(&display_mutex, msg);
            thread::sleep(Duration::from_secs(1));
            panic!(msg)
        }
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
                        if let Some((rpm, _)) = checked_rpm_and_new_count(&a, &b) {
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
