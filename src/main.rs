mod ble;
mod buttons;
mod cycle_tree;
mod display;
mod fit;
mod gps;
mod inky_phat;
#[cfg(feature = "simulator")]
mod inky_phat_simulator;
mod memory_lcd;
#[cfg(feature = "simulator")]
mod memory_lcd_simulator;
mod peripherals;
mod telemetry_db;
mod telemetry_server;
mod utils;
mod workout;

use ble::{
    csc_measurement,
    csc_measurement::{
        checked_crank_rpm_and_new_count, checked_wheel_rpm_and_new_count, parse_csc_measurement,
        CscMeasurement,
    },
    cycling_power_measurement::{parse_cycling_power_measurement, CyclingPowerMeasurement},
    heart_rate_measurement::parse_hrm,
};
use btleplug::api::Central;
use btleplug::bluez::manager::Manager;
use peripherals::{cadence::Cadence, hrm, hrm::Hrm, kickr, kickr::Kickr, speed::Speed};
use std::collections::BTreeSet;
use std::env;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use workout::{create_big_start_interval, ramp_test, single_value};

// TODO:  Allow calibration
// In meters
const WHEEL_CIRCUMFERENCE: f32 = 2.105;

#[derive(Clone)]
enum OrExit<T> {
    NotExit(T),
    Exit,
}

enum Location {
    Indoor(workout::Workout),
    Outdoor,
}

impl<T: std::fmt::Display> std::fmt::Display for OrExit<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            OrExit::NotExit(t) => write!(f, "{}", t),
            OrExit::Exit => write!(f, "Exit"),
        }
    }
}

pub fn main() {
    env_logger::init();

    let args: BTreeSet<String> = env::args().collect();
    let is_version_mode = args.contains("-v") || args.contains("--version");

    if is_version_mode {
        // TODO: It might be handy to put this on the display
        println!("{}", git_version::git_version!());
    } else {
        let db = telemetry_db::open_default().unwrap();

        // Serve our telemetry data
        let server = telemetry_server::TelemetryServer::new(db.clone());

        // Create Our Display
        let mut display = display::Display::new();

        // Create our Buttons
        // TODO: Simulate these, so we can run everything on desktop in
        // simulator mode.
        let mut buttons = buttons::Buttons::new();

        // TODO: Select Enums
        use OrExit::{Exit, NotExit};
        use SelectionTree::{Leaf, Node};
        let workout_name = selection_tree(
            &mut display,
            &mut buttons,
            vec![
                Node(("Zenia".to_string(), vec![Leaf(NotExit("100W"))])),
                Node((
                    "Nathan".to_string(),
                    vec![
                        Leaf(NotExit("Outdoor")),
                        Node((
                            "Fixed".to_string(),
                            vec![
                                Leaf(NotExit("145W")),
                                Leaf(NotExit("150W")),
                                Leaf(NotExit("155W")),
                                Leaf(NotExit("160W")),
                                Node((
                                    "More".to_string(),
                                    vec![
                                        Leaf(NotExit("165W")),
                                        Leaf(NotExit("170W")),
                                        Leaf(NotExit("175W")),
                                        Leaf(NotExit("180W")),
                                        Leaf(NotExit("185W")),
                                    ],
                                )),
                            ],
                        )),
                        Leaf(NotExit("Ramp")),
                        Leaf(NotExit("1st Big Interval")),
                    ],
                )),
                Node((
                    "Tests".to_string(),
                    vec![
                        Leaf(NotExit("GPS Only")),
                        Leaf(NotExit("GPS & HR")),
                        Leaf(NotExit("P/H/70W")),
                        Leaf(NotExit("P/H/Ramp")),
                    ],
                )),
                Leaf(Exit),
            ],
        );

        let workout_name = match workout_name {
            Exit => {
                display.render_msg("Goodbye");
                // TODO: Set this up in a way that doesn't require manual drops
                drop(db);
                drop(server);
                drop(display);
                drop(buttons);
                std::process::Command::new("sudo")
                    .arg("shutdown")
                    .arg("now")
                    .output()
                    .unwrap();
                return;
            }
            NotExit(x) => x,
        };

        let (use_hr, use_cadence, location) = match workout_name {
            "100W" => (false, false, Location::Indoor(single_value(100))),
            "Outdoor" => (true, true, Location::Outdoor),
            "145W" => (true, true, Location::Indoor(single_value(145))),
            "150W" => (true, true, Location::Indoor(single_value(150))),
            "155W" => (true, true, Location::Indoor(single_value(155))),
            "160W" => (true, true, Location::Indoor(single_value(160))),
            "165W" => (true, true, Location::Indoor(single_value(165))),
            "170W" => (true, true, Location::Indoor(single_value(170))),
            "175W" => (true, true, Location::Indoor(single_value(175))),
            "180W" => (true, true, Location::Indoor(single_value(180))),
            "185W" => (true, true, Location::Indoor(single_value(185))),
            "Ramp" => (true, true, Location::Indoor(ramp_test(120))),
            "1st Big Interval" => (
                true,
                true,
                Location::Indoor(create_big_start_interval(
                    (Duration::from_secs(300), 140),
                    14,
                    Duration::from_secs(150),
                    (Duration::from_secs(60), 320),
                    (Duration::from_secs(90), 120),
                    Some(160),
                )),
            ),
            "P/H/70W" => (true, false, Location::Indoor(single_value(70))),
            "P/H/Ramp" => (true, false, Location::Indoor(ramp_test(90))),
            "GPS Only" => (false, false, Location::Outdoor),
            "GPS & HR" => (true, false, Location::Outdoor),
            _ => panic!("Unexpected workout_name!"),
        };

        // We want instant, because we want this to be monotonic. We don't want
        // clock drift/corrections to cause events to be processed out of order.
        let start = Instant::now();

        display.set_start(Some(start));

        // Create Our Display
        let display_mutex = Arc::new(Mutex::new(display));

        // This won't fail unless the clock is before epoch, which sounds like a
        // bigger problem
        // TODO: However, there's no guarantee that this value doesn't go _backwards_, which means
        // sessions can be recorded out of order (this has happened).  We could use `max(now,
        // last_session.key+1)`, but that means that one very late clock ruins the ability to
        // determine when they were captured from the timestamp (at which point a monotonic counter
        // makes as much or more sense).
        let session_key = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        lock_and_show(&display_mutex, &format!("Running {}", workout_name));

        let _gps = if let Location::Outdoor = location {
            let mut gps =
                or_crash_with_msg(&display_mutex, gps::Gps::new().ok(), "Couldn't setup GPS!");
            let db_gps = db.clone();
            let display_mutex_for_gps = display_mutex.clone();
            gps.on_update(Box::new(move |s| {
                let mut display = display_mutex_for_gps.lock().unwrap();
                match s {
                    nmea0183::ParseResult::GGA(Some(_)) => display.set_gps_fix(true),
                    nmea0183::ParseResult::GGA(None) => display.set_gps_fix(false),
                    _ => (),
                };
                db_gps
                    .insert(
                        session_key,
                        start.elapsed(),
                        telemetry_db::Notification::Gps(s),
                    )
                    .unwrap();
            }));
            lock_and_show(&display_mutex, &format!("GPS Ready"));
            Some(gps)
        } else {
            None
        };

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

        // We need to bind to keep our speed peripheral until the end of the scope
        let _speed = if let Location::Outdoor = location {
            // Connect to Speed meter and print its raw notifications
            let speed_measure = or_crash_with_msg(
                &display_mutex,
                Speed::new(central.clone()).ok().and_then(|x| x),
                "Could not connect to Speed Measure!",
            );

            let mut o_last_speed_measure: Option<CscMeasurement> = None;
            let mut wheel_count = 0;
            let db_speed_measure = db.clone();
            let display_mutex_speed = display_mutex.clone();
            speed_measure.on_notification(Box::new(move |n| {
                let elapsed = start.elapsed();
                let csc_measure = parse_csc_measurement(&n.value);
                let r =
                    checked_wheel_rpm_and_new_count(o_last_speed_measure.as_ref(), &csc_measure);
                if let Some((wheel_rpm, new_wheel_count)) = r {
                    wheel_count = wheel_count + new_wheel_count;
                    let mut display = display_mutex_speed.lock().unwrap();
                    display.update_speed(Some(wheel_rpm as f32 * WHEEL_CIRCUMFERENCE / 60.0));
                    display.update_distance(wheel_count as f64 * WHEEL_CIRCUMFERENCE as f64);
                }
                o_last_speed_measure = Some(csc_measure);
                db_speed_measure
                    .insert(
                        session_key,
                        elapsed,
                        telemetry_db::Notification::Ble((n.uuid, n.value)),
                    )
                    .unwrap();
            }));
            lock_and_show(&display_mutex, &"Setup Complete for Speed Monitor");
            Some(speed_measure)
        } else {
            None
        };

        // We need to bind to keep our hrm until the end of the scope
        let _hrm = if use_hr {
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
                db_hrm
                    .insert(
                        session_key,
                        elapsed,
                        telemetry_db::Notification::Ble((n.uuid, n.value)),
                    )
                    .unwrap();
            }));
            lock_and_show(&display_mutex, &"Setup Complete for Heart Rate Monitor");
            Some(hrm)
        } else {
            None
        };

        let kickr_and_handle = if let Location::Indoor(workout) = location {
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
                if n.uuid == kickr::MEASURE_UUID {
                    let mut display = display_mutex_kickr.lock().unwrap();
                    let power_reading = parse_cycling_power_measurement(&n.value);
                    let o_new_acc_torque = o_last_power_reading
                        .as_ref()
                        .and_then(|x| x.new_accumulated_torque(&power_reading));
                    if let Some(new_acc_torque) = o_new_acc_torque {
                        acc_torque = acc_torque + new_acc_torque;
                        display.update_external_energy(2.0 * std::f64::consts::PI * acc_torque);
                    }
                    display.update_power(Some(power_reading.instantaneous_power));
                    o_last_power_reading = Some(power_reading);
                    let elapsed = start.elapsed();
                    db_kickr
                        .insert(
                            session_key,
                            elapsed,
                            telemetry_db::Notification::Ble((n.uuid, n.value)),
                        )
                        .unwrap();
                } else {
                    println!("Non-power notification from kickr: {:?}", n);
                }
            }));

            // run our workout
            // Our workout will drop the closure after the workout ends (last
            // power_set) and if we don't hold a reference to our kickr, it will
            // be dropped along with the closure.  Dropping the kickr ends all
            // of its subscriptions.
            // TODO: Maybe all workouts should have an explicit end, rather than
            // a tail?  That would make this more intuitive.  Then at the end of
            // the workout, the program exits (and systemd restarts it).
            let kickr = Arc::new(kickr);
            let kickr_for_workout = kickr.clone();
            let workout_handle = workout.run(Instant::now(), move |p| {
                kickr_for_workout.set_power(p).unwrap();
            });

            lock_and_show(&display_mutex, &"Setup Complete for Kickr");
            Some((workout_handle, kickr))
        } else {
            None
        };

        // We need to bind to keep our cadence peripheral until the end of the scope
        let _cadence = if use_cadence {
            // Connect to Cadence meter and print its raw notifications
            let cadence_measure = or_crash_with_msg(
                &display_mutex,
                Cadence::new(central.clone()).ok().and_then(|x| x),
                "Could not connect to Cadence Measure!",
            );

            let mut o_last_cadence_measure: Option<CscMeasurement> = None;
            let mut crank_count = 0;
            let db_cadence_measure = db.clone();
            let display_mutex_cadence = display_mutex.clone();
            cadence_measure.on_notification(Box::new(move |n| {
                let elapsed = start.elapsed();
                let csc_measure = parse_csc_measurement(&n.value);
                let r =
                    checked_crank_rpm_and_new_count(o_last_cadence_measure.as_ref(), &csc_measure);
                if let Some((rpm, new_crank_count)) = r {
                    crank_count = crank_count + new_crank_count;
                    let mut display = display_mutex_cadence.lock().unwrap();
                    display.update_cadence(Some(rpm as u8));
                    display.update_crank_count(crank_count);
                }
                o_last_cadence_measure = Some(csc_measure);
                db_cadence_measure
                    .insert(
                        session_key,
                        elapsed,
                        telemetry_db::Notification::Ble((n.uuid, n.value)),
                    )
                    .unwrap();
            }));
            lock_and_show(&display_mutex, &"Setup Complete for Cadence Monitor");
            Some(cadence_measure)
        } else {
            None
        };

        let m_will_exit = Arc::new(Mutex::new(false));
        let m_will_exit_for_button = m_will_exit.clone();
        buttons.on_hold(
            buttons::Button::ButtonA,
            Duration::from_secs(5),
            Box::new(move || {
                let mut will_exit = m_will_exit_for_button.lock().unwrap();
                *will_exit = true;
            }),
        );

        // Update it every second
        let display_mutex_for_render = display_mutex.clone();
        let m_will_exit_for_render = m_will_exit.clone();
        let render_handle = thread::spawn(move || loop {
            {
                if *m_will_exit_for_render.lock().unwrap() {
                    break;
                }
            };
            {
                let mut display = display_mutex_for_render.lock().unwrap();
                display.render();
            }
            thread::sleep(Duration::from_millis(100));
        });

        if let Some((mut wh, _)) = kickr_and_handle {
            wh.exit();
        }
        render_handle.join().unwrap();
        lock_and_show(&display_mutex, &"Goodbye");
    }
}

#[derive(Clone)]
enum SelectionTree<T> {
    Leaf(T),
    Node((String, Vec<SelectionTree<T>>)),
}

impl<T: std::fmt::Display> std::fmt::Display for SelectionTree<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            SelectionTree::Leaf(t) => write!(f, "{}", t),
            SelectionTree::Node((label, _)) => write!(f, "{}", label),
        }
    }
}

fn selection_tree<O: std::fmt::Display + Clone>(
    mut display: &mut display::Display,
    mut buttons: &mut buttons::Buttons,
    tree: Vec<SelectionTree<O>>,
) -> O {
    let mut t = tree;
    loop {
        match selection(&mut display, &mut buttons, &t) {
            SelectionTree::Node((_, selected_tree)) => {
                t = selected_tree;
            }
            SelectionTree::Leaf(x) => {
                break x;
            }
        }
    }
}

fn selection<O: std::fmt::Display + Clone>(
    display: &mut display::Display,
    buttons: &mut buttons::Buttons,
    options: &Vec<O>,
) -> O {
    if options.len() < 1 || options.len() > 5 {
        panic!("Unsupported selection length!");
    }

    let choice = Arc::new(Mutex::new(None));
    use buttons::Button;
    let bs = vec![
        Button::ButtonE,
        Button::ButtonD,
        Button::ButtonC,
        Button::ButtonB,
        Button::ButtonA,
    ];

    for i in 0..options.len() {
        let choice_button = choice.clone();
        buttons.on_press(
            bs[i],
            Box::new(move || {
                let mut choice = choice_button.lock().unwrap();
                if let None = *choice {
                    *choice = Some(i);
                }
            }),
        );
    }

    let strings: Vec<String> = options.iter().map(|x| format!("{}", x)).collect();
    display.render_options(&strings.iter().map(|x| &**x).collect());

    let index = loop {
        {
            let or = choice.lock().unwrap();
            if let Some(r) = *or {
                break r;
            }
        }
        thread::sleep(Duration::from_millis(15));
    };

    for b in bs {
        buttons.clear_handlers(b);
    }

    options[index].clone()
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

fn db_session_to_fit(db: &telemetry_db::TelemetryDb, session_key: u64) -> Vec<u8> {
    let mut last_power: Option<u16> = None;
    let mut last_cadence_csc_measurement: Option<CscMeasurement> = None;
    let mut last_wheel_csc_measurement: Option<CscMeasurement> = None;
    let mut wheel_count = 0;
    let mut record: Option<fit::FitRecord> = None;
    let mut records = Vec::new();
    let empty_record = |t| fit::FitRecord {
        seconds_since_unix_epoch: t,
        power: None,
        heart_rate: None,
        cadence: None,
        latitude: None,
        longitude: None,
        altitude: None,
        distance: None,
        speed: None,
    };

    for x in db.get_session_entries(session_key) {
        if let Ok((d, value)) = x {
            let seconds_since_unix_epoch = (session_key + d.as_secs()) as u32;
            let mut r = match record {
                Some(mut r) => {
                    if r.seconds_since_unix_epoch == seconds_since_unix_epoch {
                        r
                    } else {
                        if let None = r.power {
                            r.power = last_power;
                        }
                        records.push(r);
                        empty_record(seconds_since_unix_epoch)
                    }
                }
                None => empty_record(seconds_since_unix_epoch),
            };

            record = Some(match value {
                telemetry_db::Notification::Gps(nmea0183::ParseResult::GGA(Some(gga))) => {
                    r.latitude = Some(gga.latitude.as_f64());
                    r.longitude = Some(gga.longitude.as_f64());
                    r.altitude = Some(gga.altitude.meters);
                    r
                }
                telemetry_db::Notification::Gps(_) => r,
                telemetry_db::Notification::Ble((hrm::MEASURE_UUID, v)) => {
                    r.heart_rate = Some(parse_hrm(&v).bpm as u8);
                    r
                }
                telemetry_db::Notification::Ble((kickr::MEASURE_UUID, v)) => {
                    let p = parse_cycling_power_measurement(&v).instantaneous_power as u16;
                    last_power = Some(p);
                    r.power = Some(p);
                    r
                }
                telemetry_db::Notification::Ble((csc_measurement::MEASURE_UUID, v)) => {
                    // TODO: Clean up cloning here that supports crank and wheel
                    // data coming from different sources :/
                    // We can't tell if this reading support just one or both,
                    // given that the CSC UUID/characterstic supports both.
                    let csc_measurement = parse_csc_measurement(&v);
                    let o_crank_rpm = checked_crank_rpm_and_new_count(
                        last_cadence_csc_measurement.as_ref(),
                        &csc_measurement,
                    )
                    .map(|x| x.0);
                    let o_wheel = checked_wheel_rpm_and_new_count(
                        last_wheel_csc_measurement.as_ref(),
                        &csc_measurement,
                    );
                    if let Some(crank_rpm) = o_crank_rpm {
                        r.cadence = Some(crank_rpm as u8);
                    }
                    if let Some((wheel_rpm, new_wheel_count)) = o_wheel {
                        r.speed = Some(wheel_rpm as f32 * WHEEL_CIRCUMFERENCE / 60.0);
                        wheel_count += new_wheel_count;
                        r.distance = Some(wheel_count as f64 * WHEEL_CIRCUMFERENCE as f64);
                    }
                    // We want to consider both the cases where we have
                    // individual devices and one that has both measures.
                    if csc_measurement.crank.is_some() {
                        last_cadence_csc_measurement = Some(csc_measurement.clone());
                    }
                    if csc_measurement.wheel.is_some() {
                        last_wheel_csc_measurement = Some(csc_measurement.clone());
                    }
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
