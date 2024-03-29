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
    cycling_power_measurement,
    cycling_power_measurement::{parse_cycling_power_measurement, CyclingPowerMeasurement},
    heart_rate_measurement::parse_hrm,
};
use btleplug::api::{Central, Manager as _, ScanFilter, Peripheral};
use btleplug::platform::Manager;
use btleplug::Error::DeviceNotFound;
use peripherals::{kickr, hrm, assioma, speed, cadence};
use std::collections::BTreeSet;
use std::env;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use futures::stream::StreamExt;
use workout::{create_big_start_interval, ramp_test, single_value};

// TODO:  Allow calibration
// In meters
const WHEEL_CIRCUMFERENCE: f32 = 2.105;

#[derive(Clone)]
enum OrExit<T> {
    NotExit(T),
    Exit,
}

#[derive(Clone, Debug)]
enum SetupNextStep {
    TryAgain,
    ContinueWithout,
    Crash,
}

#[derive(Clone, Debug)]
enum IgnorableError {
    Ignore,
    Exit,
}

#[derive(Clone)]
struct SelectedDevices {
    assioma: bool,
    cadence: bool,
    gps: bool,
    hr: bool,
    kickr: bool,
    speed: bool,
}

#[tokio::main]
pub async fn main() {
    env_logger::init();

    let args: BTreeSet<String> = env::args().collect();
    let is_version_mode = args.contains("-v") || args.contains("--version");
    // TODO: There's probably an alternate way to do this with nix
    let version = "TODO"; // git_version::git_version!();

    if is_version_mode {
        // TODO: It might be handy to put this on the display
        println!("{}", version);
    } else {
        let db = telemetry_db::open_default().unwrap();

        // Serve our telemetry data
        let server = telemetry_server::TelemetryServer::new(db.clone());

        // Create Our Display
        let mut display = display::Display::new(version.to_string());

        // Create our Buttons
        // TODO: Simulate these, so we can run everything on desktop in
        // simulator mode.
        let mut buttons = buttons::Buttons::new();

        // TODO: Select Enums
        use OrExit::{Exit, NotExit};
        use SelectionTreeValue::{Leaf, Node};
        #[cfg(not(feature = "simulator"))]
        let devices = selection_tree(
            &mut display,
            &mut buttons,
            vec![
                SelectionTree {
                    label: "Zenia".to_string(),
                    value: Leaf(NotExit(SelectedDevices {
                        assioma: false,
                        cadence: true,
                        gps: false,
                        hr: false,
                        kickr: true,
                        speed: false,
                    })),
                },
                SelectionTree {
                    label: "Nathan Outdoor".to_string(),
                    value: Leaf(NotExit(SelectedDevices {
                        assioma: true,
                        cadence: false,
                        gps: true,
                        hr: true,
                        kickr: false,
                        speed: true,
                    })),
                },
                SelectionTree {
                    label: "Nathan Indoor".to_string(),
                    value: Leaf(NotExit(SelectedDevices {
                        assioma: true,
                        cadence: false,
                        gps: false,
                        hr: true,
                        kickr: true,
                        speed: false,
                    })),
                },
                SelectionTree {
                    label: "Exit".to_string(),
                    value: Leaf(Exit),
                },
            ],
            &"Choose profile",
        );

        #[cfg(not(feature = "simulator"))]
        let devices = match devices {
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

        #[cfg(not(feature = "simulator"))]
        let workout = selection_tree(
            &mut display,
            &mut buttons,
            vec![
                SelectionTree {
                    label: "Fixed".to_string(),
                    value: Node(vec![
                        SelectionTree {
                            label: "100W".to_string(),
                            value: Leaf(single_value(100)),
                        },
                        SelectionTree {
                            label: "135W".to_string(),
                            value: Leaf(single_value(135)),
                        },
                        SelectionTree {
                            label: "140W".to_string(),
                            value: Leaf(single_value(140)),
                        },
                        SelectionTree {
                            label: "145W".to_string(),
                            value: Leaf(single_value(145)),
                        },
                        SelectionTree {
                            label: "More".to_string(),
                            value: Node(vec![
                                SelectionTree {
                                    label: "150W".to_string(),
                                    value: Leaf(single_value(150)),
                                },
                                SelectionTree {
                                    label: "155W".to_string(),
                                    value: Leaf(single_value(155)),
                                },
                                SelectionTree {
                                    label: "160W".to_string(),
                                    value: Leaf(single_value(160)),
                                },
                                SelectionTree {
                                    label: "165W".to_string(),
                                    value: Leaf(single_value(165)),
                                },
                                SelectionTree {
                                    label: "170W".to_string(),
                                    value: Leaf(single_value(170)),
                                },
                            ]),
                        },
                    ]),
                },
                SelectionTree {
                    label: "Ramp".to_string(),
                    value: Leaf(ramp_test(120)),
                },
                SelectionTree {
                    label: "1st Big Interval".to_string(),
                    value: Leaf(create_big_start_interval(
                        (Duration::from_secs(300), 140),
                        14,
                        Duration::from_secs(150),
                        (Duration::from_secs(60), 320),
                        (Duration::from_secs(90), 120),
                        Some(160),
                    )),
                },
            ],
            &"Choose workout",
        );

        #[cfg(feature = "simulator")]
        let devices = SelectedDevices {
                        assioma: true,
                        cadence: false,
                        gps: false,
                        hr: true,
                        kickr: false,
                        speed: false,
                    };
        #[cfg(feature = "simulator")]
        let workout = single_value(100);

        // We want instant, because we want this to be monotonic. We don't want
        // clock drift/corrections to cause events to be processed out of order.
        let start = Instant::now();

        display.set_start(Some(start));

        // To make sure we never go backwards (the real time clock is not
        // reliable especially after a crash or if wifi is unavailable), we make
        // the session key larger than the most recent previous one.
        let session_key = u64::max(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                // This won't fail unless the clock is before epoch, which
                // sounds like a bigger problem
                .unwrap()
                .as_secs(),
            // TODO: one very late clock ruins the ability to determine when
            // they were captured from the timestamp.
            db.get_most_recent_session().unwrap().unwrap_or(0) + 1,
        );

        let mut o_gps =
            user_connect_or_skip(&mut display, &mut buttons, devices.gps, "GPS", || {
                gps::Gps::new()
            });

        // User prompts don't really help us much here, because this is a pretty
        // hopeless case--pretty much everything uses bluetooth!
        display.render_msg("Setting up Bluetooth");
        let central = or_crash_with_msg(
            &mut display,
            setup_ble_and_discover_devices()
                .await
                // Result to Option
                // TODO: Loses original error
                .ok()
                //aka flatten: Option<Option<T>> -> Option<T>
                .and_then(|x| x),
            "Couldn't setup bluetooth!",
        );
        display.render_msg("Connecting to Devices.");

        // TODO: Can't use ?
        let mut o_speed =
           if devices.speed {
               match squish_error(speed::connect(&central).await) {
                   Ok(speed) => Some(speed),
                   Err(e) => {
                       println!("{:?}", e);
                       match prompt_ignore_or_exit(
                           &mut display,
                           &mut buttons,
                           "Speed connect error."
                       ) {
                           IgnorableError::Ignore => None,
                           IgnorableError::Exit => {
                               crash_with_msg(&mut display, "Speed connect error.")
                           }
                       }
                   }
               }
           } else {
               None
           };

        // TODO: Can't use ?
        let mut o_hrm =
           if devices.hr {
               match squish_error(hrm::connect(&central).await) {
                   Ok(hrm) => Some(hrm),
                   Err(e) => {
                       println!("{:?}", e);
                       match prompt_ignore_or_exit(
                           &mut display,
                           &mut buttons,
                           "HR Monitor connect error."
                       ) {
                           IgnorableError::Ignore => None,
                           IgnorableError::Exit => {
                               crash_with_msg(&mut display, "HR Monitor connect error.")
                           }
                       }
                   }
               }
           } else {
               None
           };

        // TODO: Can't use ?
        let mut o_kickr =
           if devices.kickr {
               Some(or_crash_with_msg(&mut display, kickr::connect(&central).await.unwrap(), "Kickr was requested but not found."))
           } else {
               None
           };

        // TODO: Can't use ?
        let mut o_assioma =
           if devices.assioma {
               Some(or_crash_with_msg(&mut display, assioma::connect(&central).await.unwrap(), "Assioma was requested but not found."))
           } else {
               None
           };

        // TODO: Can't use ?
        let mut o_cadence =
           if devices.cadence {
               match squish_error(cadence::connect(&central).await) {
                   Ok(cadence) => Some(cadence),
                   Err(e) => {
                       println!("{:?}", e);
                       match prompt_ignore_or_exit(
                           &mut display,
                           &mut buttons,
                           "Cadence connect error."
                       ) {
                           IgnorableError::Ignore => None,
                           IgnorableError::Exit => {
                               crash_with_msg(&mut display, "Cadence connect error.")
                           }
                       }
                   }
               }
           } else {
               None
           };

        // We now need a mutex, so we can share the display out to multiple
        // peripherals
        let display_mutex = Arc::new(Mutex::new(display));

        // Need to make sure we don't consume the optional, or it will be
        // dropped prematurely
        for gps in &mut o_gps {
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
        }

        // Need to make sure we don't consume the optional, or it will be
        // dropped prematurely
        for speed_measure in &mut o_speed {
            let mut o_last_speed_measure: Option<CscMeasurement> = None;
            let mut wheel_count = 0;
            let db_speed_measure = db.clone();
            let display_mutex_speed = display_mutex.clone();
            // TODO: Cannot use ? in async block that returns ()
            let mut notifications = speed_measure.notifications().await.unwrap();
            tokio::spawn(async move {
                while let Some(n) = notifications.next().await {
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
                }
            });
            lock_and_show(&display_mutex, &"Setup Complete for Speed Monitor");
        }

        // Need to make sure we don't consume the optional, or it will be
        // dropped prematurely
        for hrm in &mut o_hrm {
            let db_hrm = db.clone();
            let display_mutex_hrm = display_mutex.clone();
            // TODO: Cannot use ? in async block that returns ()
            let mut notifications = hrm.notifications().await.unwrap();
            tokio::spawn(async move {
                while let Some(n) = notifications.next().await {
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
                };
            });
            lock_and_show(&display_mutex, &"Setup Complete for Heart Rate Monitor");
        }

        let use_assioma = devices.assioma;

        // Need to make sure we don't consume the optional, or it will be
        // dropped prematurely
        for (kickr, _) in &mut o_kickr {
            let db_kickr = db.clone();
            let display_mutex_kickr = display_mutex.clone();
            let mut o_last_power_reading: Option<CyclingPowerMeasurement> = None;
            let mut acc_torque = 0.0;
            let mut notifications = kickr.notifications().await.unwrap();
            tokio::spawn(async move {
                while let Some(n) = notifications.next().await {
                    if n.uuid == kickr::MEASURE_UUID {
                        let mut display = display_mutex_kickr.lock().unwrap();
                        let power_reading = parse_cycling_power_measurement(&n.value);
                        let o_new_acc_torque = o_last_power_reading
                            .as_ref()
                            .and_then(|x| x.new_accumulated_torque(&power_reading));
                        if let Some(new_acc_torque) = o_new_acc_torque {
                            acc_torque = acc_torque + new_acc_torque;
                            //TODO: The display should be able to accept a "wheel" and "crank" external
                            //energy field separately.  Right now for testing we just disable the
                            //KICKR's output to the display.
                            if !use_assioma {
                                display.update_external_energy(2.0 * std::f64::consts::PI * acc_torque);
                            }
                        }
                        //TODO: The display should be able to accept a "wheel" and "crank" power field
                        //separately.  Right now for testing we just disable the KICKR's output to the
                        //display.
                        if !use_assioma {
                            display.update_power(Some(power_reading.instantaneous_power));
                        }
                        o_last_power_reading = Some(power_reading);
                        let elapsed = start.elapsed();
                        //TODO: Not exactly sure how to handle having _both_ power captures for when it
                        //comes to generating fit files.
                        if !use_assioma {
                            db_kickr
                                .insert(
                                    session_key,
                                    elapsed,
                                    telemetry_db::Notification::Ble((n.uuid, n.value)),
                                )
                                .unwrap();
                        }
                    } else {
                        println!("Non-power notification from kickr: {:?}", n);
                    }
                }
            });
            lock_and_show(&display_mutex, &"Setup Complete for Kickr");
        }

        // Need to make sure we don't consume the optional, or it will be
        // dropped prematurely
        for assioma in &mut o_assioma {
            let mut o_last_power_measure: Option<CyclingPowerMeasurement> = None;
            let mut crank_count = 0;
            let mut acc_torque = 0.0;
            let db_power_measure = db.clone();
            let display_mutex_assioma = display_mutex.clone();
            // TODO: Cannot use ? in async block that returns ()
            let mut notifications = assioma.notifications().await.unwrap();
            tokio::spawn(async move {
                while let Some(n) = notifications.next().await {
                    let elapsed = start.elapsed();
                    let power_measure = parse_cycling_power_measurement(&n.value);
                    let r = cycling_power_measurement::checked_crank_rpm_and_new_count(
                        o_last_power_measure.as_ref(),
                        &power_measure,
                    );
                    let mut display = display_mutex_assioma.lock().unwrap();
                    if let Some((rpm, new_crank_count)) = r {
                        crank_count = crank_count + new_crank_count;
                        display.update_cadence(Some(rpm as u8));
                        display.update_crank_count(crank_count);
                    }
                    let o_new_acc_torque = o_last_power_measure
                        .as_ref()
                        .and_then(|x| x.new_accumulated_torque(&power_measure));
                    if let Some(new_acc_torque) = o_new_acc_torque {
                        acc_torque = acc_torque + new_acc_torque;
                        display.update_external_energy(2.0 * std::f64::consts::PI * acc_torque);
                    }
                    display.update_power(Some(power_measure.instantaneous_power));
                    o_last_power_measure = Some(power_measure);
                    db_power_measure
                        .insert(
                            session_key,
                            elapsed,
                            telemetry_db::Notification::Ble((n.uuid, n.value)),
                        )
                        .unwrap();
                }
            });
            lock_and_show(&display_mutex, &"Setup Complete for Assioma Pedals!");
        }

        // Need to make sure we don't consume the optional, or it will be
        // dropped prematurely
        for cadence_measure in &mut o_cadence {
            let mut o_last_cadence_measure: Option<CscMeasurement> = None;
            let mut crank_count = 0;
            let db_cadence_measure = db.clone();
            let display_mutex_cadence = display_mutex.clone();
            let mut notifications = cadence_measure.notifications().await.unwrap();
            tokio::spawn(async move {
                while let Some(n) = notifications.next().await {
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
                }
            });
            lock_and_show(&display_mutex, &"Setup Complete for Cadence Monitor");
        }

        // run our workout
        // Our workout will drop the closure after the workout ends (last
        // power_set) and if we don't hold a reference to our kickr, it will be
        // dropped along with the closure.  Dropping the kickr ends all of its
        // subscriptions.
        // TODO: Maybe all workouts should have an explicit end, rather than a
        // tail?  That would make this more intuitive.  Then at the end of the
        // workout, the program exits (and systemd restarts it).

        // TODO: It's dumb that were managing these two separate mutexes (power
        // target and display).  The target should just be private state of the
        // display, so if we modify the workout's offset or, that later sets the
        // power, which modifies the internal state of the display, which is
        // reflected on the next display render.
        let power_target_mutex = Arc::new(Mutex::new(0));

        let power_target_mutex_workout = power_target_mutex.clone();

        let o_kickr_for_workout = o_kickr.clone();
        let display_mutex_workout = display_mutex.clone();
        let mut workout_handle = workout.run(Instant::now(), move |p| {
            // Update our power target used by the display, and update the
            // display immediately
            {
                let mut power_target = power_target_mutex_workout.lock().unwrap();
                *power_target = p;
                let mut display = display_mutex_workout.lock().unwrap();
                display.set_page(display::Page::PowerTrack(p as i16));
            }

            // TODO: got to be a better way than this!
            let o_kickr_for_workout = o_kickr_for_workout.clone();
            async move {
                // If there's a connected Kickr, set its ERG mode power
                for (kickr, target_power) in o_kickr_for_workout.iter() {
                    kickr::set_power(kickr, target_power, p).await.unwrap();
                }
            }
        });

        // TODO: The Combo of Buttons and Display should make up a sort of
        // "UserInterface" that hides the buttons (this would make using the
        // simulator much easier, for example).
        let display_mutex_standard_page = display_mutex.clone();
        buttons.on_press(
            buttons::Button::ButtonE,
            Box::new(move || {
                let mut display = display_mutex_standard_page.lock().unwrap();
                display.set_page(display::Page::Standard);
            }),
        );

        // TODO: Like many other things, this should be encapsulated in some
        // sort of User Interface concept that understands both inputs (buttons)
        // and outputs (screens)
        // TODO: Quite a lot of repetition here to ensure that changes to the
        // target refect immediately.

        let power_target_mutex_power_track_page = power_target_mutex.clone();
        let display_mutex_power_track_page = display_mutex.clone();
        buttons.on_press(
            buttons::Button::ButtonD,
            Box::new(move || {
                let mut display = display_mutex_power_track_page.lock().unwrap();
                let power = power_target_mutex_power_track_page.lock().unwrap();
                // TODO: This should be configurable
                display.set_page(display::Page::PowerTrack(*power as i16));
            }),
        );

        let workout_state = workout_handle.state.clone();
        buttons.on_hold(
            buttons::Button::ButtonE,
            Duration::from_secs(2),
            Box::new(move || workout::add_offset(&workout_state, -5)),
        );

        let workout_state = workout_handle.state.clone();
        buttons.on_hold(
            buttons::Button::ButtonD,
            Duration::from_secs(2),
            Box::new(move || workout::add_offset(&workout_state, 5)),
        );

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

        // TODO: Idealy, the end of a workout ends the program
        render_handle.join().unwrap();
        workout_handle.exit().await;
        lock_and_show(&display_mutex, &"Goodbye");
    }
}

#[derive(Clone)]
pub struct SelectionTree<T> {
    label: String,
    value: SelectionTreeValue<T>,
}

#[derive(Clone)]
enum SelectionTreeValue<T> {
    Leaf(T),
    Node(Vec<SelectionTree<T>>),
}

impl<T> std::fmt::Display for SelectionTree<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.label)
    }
}

// TODO: Sets of choices should also likely have labels, like "choose your
// favorite breakfast food:"
fn selection_tree<O: Clone>(
    mut display: &mut display::Display,
    mut buttons: &mut buttons::Buttons,
    tree: Vec<SelectionTree<O>>,
    label: &str,
) -> O {
    let mut t = tree;
    loop {
        match selection(&mut display, &mut buttons, &t, label).value {
            SelectionTreeValue::Node(selected_tree) => {
                t = selected_tree;
            }
            SelectionTreeValue::Leaf(x) => {
                break x;
            }
        }
    }
}

fn selection<O: std::fmt::Display + Clone>(
    display: &mut display::Display,
    buttons: &mut buttons::Buttons,
    options: &Vec<O>,
    label: &str,
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
    display.render_options(label, &strings.iter().map(|x| &**x).collect());

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
async fn setup_ble_and_discover_devices() -> btleplug::Result<Option<btleplug::platform::Adapter>> {
    println!("Getting Manager...");
    let manager = Manager::new().await?;

    println!("Getting Adapters...");
    let adapters = manager.adapters().await.unwrap();

    match adapters.into_iter().next() {
        Some(central) => {
            println!("Starting Scan...");
            central.start_scan(ScanFilter::default()).await?;

            tokio::time::sleep(Duration::from_secs(5)).await;

            println!("Stopping scan...");
            central.stop_scan().await?;
            Ok(Some(central))
        }
        None => Ok(None),
    }
}

fn squish_error<T>(x: btleplug::Result<Option<T>>) -> btleplug::Result<T> {
    match x {
        Ok(None) => Err(DeviceNotFound),
        Ok(Some(y)) => Ok(y),
        Err(e) => Err(e),
    }
}

fn user_connect_or_skip<T, E: std::fmt::Debug, F: Fn() -> Result<T, E>>(
    display: &mut display::Display,
    buttons: &mut buttons::Buttons,
    in_use: bool,
    name: &str,
    f: F,
) -> Option<T> {
    let mut in_use = in_use;
    loop {
        if in_use {
            match f() {
                Ok(peripheral) => {
                    break Some(peripheral);
                }
                Err(e) => {
                    // Get this into the logs at least
                    println!("{:?}", e);
                    display.render_msg(&format!("Error Connecting to {}: {:?}", name, e));
                    thread::sleep(Duration::from_secs(1));
                    let choice = selection_tree(
                        display,
                        buttons,
                        vec![
                            SelectionTree {
                                label: "Try Again".to_string(),
                                value: SelectionTreeValue::Leaf(SetupNextStep::TryAgain),
                            },
                            SelectionTree {
                                label: "Continue Without".to_string(),
                                value: SelectionTreeValue::Leaf(SetupNextStep::ContinueWithout),
                            },
                            SelectionTree {
                                label: "Exit".to_string(),
                                value: SelectionTreeValue::Leaf(SetupNextStep::Crash),
                            },
                        ],
                        &format!("{} failed to connect", name),
                    );
                    match choice {
                        SetupNextStep::TryAgain => {
                            display.render_msg(&format!("Retrying {}", name));
                            thread::sleep(Duration::from_secs(1));
                        }
                        SetupNextStep::ContinueWithout => {
                            in_use = false;
                        }
                        SetupNextStep::Crash => crash_with_msg(display, "Goodbye"),
                    }
                }
            }
        } else {
            break None;
        }
    }
}

fn prompt_ignore_or_exit(
    display: &mut display::Display,
    buttons: &mut buttons::Buttons,
    msg: &str,
) -> IgnorableError {
    display.render_msg(&format!("{}", msg));
    thread::sleep(Duration::from_secs(1));
    selection_tree(
        display,
        buttons,
        vec![
            SelectionTree {
                label: "Ignore and Continue".to_string(),
                value: SelectionTreeValue::Leaf(IgnorableError::Ignore),
            },
            SelectionTree {
                label: "Exit".to_string(),
                value: SelectionTreeValue::Leaf(IgnorableError::Exit),
            },
        ],
        &format!("{}", msg),
    )
}

fn lock_and_show(display_mutex: &Arc<Mutex<display::Display>>, msg: &str) {
    let mut display = display_mutex.lock().unwrap();
    display.render_msg(msg);
}

fn crash_with_msg<T>(display: &mut display::Display, msg: &'static str) -> T {
    display.render_msg(msg);
    thread::sleep(Duration::from_secs(1));
    panic!("{}", msg)
}

fn or_crash_with_msg<T>(display: &mut display::Display, x: Option<T>, msg: &'static str) -> T {
    match x {
        Some(y) => y,
        None => crash_with_msg(display, msg),
    }
}

fn or_crash_and_lock_with_msg<T>(
    display_mutex: &Arc<Mutex<display::Display>>,
    x: Option<T>,
    msg: &'static str,
) -> T {
    match x {
        Some(y) => y,
        None => {
            let mut display = display_mutex.lock().unwrap();
            crash_with_msg(&mut display, msg)
        }
    }
}

fn db_sessions_to_fit<I: Iterator<Item = u64>>(
    db: &telemetry_db::TelemetryDb,
    session_keys: I,
) -> sled::Result<Vec<u8>> {
    // TODO: Ideally we could stay lazy through this whole process and
    // fit::to_file would accept any generic iterator
    let fit_records: sled::Result<Vec<fit::FitRecord>> = session_keys
        .flat_map(|sk| db_session_to_fit_records(db, sk))
        .collect();
    fit_records.map(|frs| fit::to_file(&frs))
}

fn db_session_to_fit_records(
    db: &telemetry_db::TelemetryDb,
    session_key: u64,
) -> impl Iterator<Item = sled::Result<fit::FitRecord>> + '_ {
    let mut last_power_measure: Option<CyclingPowerMeasurement> = None;
    let mut last_cadence_csc_measurement: Option<CscMeasurement> = None;
    let mut last_wheel_csc_measurement: Option<CscMeasurement> = None;
    let mut wheel_count = 0;
    let mut record: Option<fit::FitRecord> = None;
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

    db.get_session_entries(session_key).filter_map(move |x| {
        match x {
            Ok((d, value)) => {
                let mut finished_record = None;
                let seconds_since_unix_epoch = (session_key + d.as_secs()) as u32;
                let mut r = match record.take() {
                    Some(mut r) => {
                        if r.seconds_since_unix_epoch == seconds_since_unix_epoch {
                            r
                        } else {
                            if let None = r.power {
                                r.power = last_power_measure
                                    .as_ref()
                                    .map(|p| p.instantaneous_power as u16);
                            }
                            finished_record = Some(r);
                            empty_record(seconds_since_unix_epoch)
                        }
                    }
                    None => empty_record(seconds_since_unix_epoch),
                };

                match value {
                    telemetry_db::Notification::Gps(nmea0183::ParseResult::GGA(Some(gga))) => {
                        r.latitude = Some(gga.latitude.as_f64());
                        r.longitude = Some(gga.longitude.as_f64());
                        r.altitude = Some(gga.altitude.meters);
                    }
                    telemetry_db::Notification::Gps(_) => (),
                    telemetry_db::Notification::Ble((hrm::MEASURE_UUID, v)) => {
                        r.heart_rate = Some(parse_hrm(&v).bpm as u8);
                    }
                    telemetry_db::Notification::Ble((assioma::MEASURE_UUID, v)) => {
                        let power_measure = parse_cycling_power_measurement(&v);
                        r.power = Some(power_measure.instantaneous_power as u16);
                        let o_crank_rpm =
                            cycling_power_measurement::checked_crank_rpm_and_new_count(
                                last_power_measure.as_ref(),
                                &power_measure,
                            )
                            .map(|x| x.0);
                        if let Some(crank_rpm) = o_crank_rpm {
                            r.cadence = Some(crank_rpm as u8);
                        }
                        last_power_measure = Some(power_measure);
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
                    }
                    _ => {
                        println!("UUID not matched");
                    }
                };

                record = Some(r);

                finished_record.map(|x| Ok(x))
            }
            Err(e) => Some(Err(e)),
        }
    })
}
