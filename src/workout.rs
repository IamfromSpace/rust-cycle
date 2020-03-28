use crate::cycle_tree::CycleTree;
use std::{
    mem,
    sync::{Arc, Mutex},
    thread,
    thread::JoinHandle,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Workout {
    ct: CycleTree<(Duration, u16)>,
    tail: Option<u16>,
}

pub struct WorkoutHandle {
    running: Arc<Mutex<bool>>,
    join_handle: Option<JoinHandle<()>>,
}

impl WorkoutHandle {
    pub fn exit(&mut self) {
        {
            let mut running = self.running.lock().unwrap();
            *running = false;
        }
        if let Some(jh) = mem::replace(&mut self.join_handle, None) {
            jh.join().unwrap();
        }
    }
}

impl Workout {
    // A workout is constructed from a cycle tree that holds how long a certain
    // amount of power should be held for, and then optionally a final power
    // that is held indefinitely at the end of the workout (defaults to 0).
    pub fn new(ct: CycleTree<(Duration, u16)>, tail: Option<u16>) -> Workout {
        Workout { ct, tail }
    }

    // This also eventually self-corrects any drift, because we always target the
    // correct total time for our changes.
    pub fn run<F: Fn(u16) + Send + 'static>(self, start: Instant, set_power: F) -> WorkoutHandle {
        // TODO: There must be a more elegant way to do this
        let running = Arc::new(Mutex::new(true));
        let running_for_thread = running.clone();
        let Workout { ct, tail } = self;
        let join_handle = Some(thread::spawn(move || {
            let mut d = Duration::from_secs(0);
            for (wait, power) in ct.into_iter() {
                // Overflow is not a consideration for the timeline of a single workout
                d = d.checked_add(wait).unwrap();
                let e = start.elapsed();
                // If duration is negative, we continue on.
                if let Some(_) = d.checked_sub(e) {
                    set_power(power);

                    // We loop and check every 50ms if we should move to the
                    // next power or if the workout is teriminated.
                    let terminate = loop {
                        thread::sleep(Duration::from_millis(50));
                        {
                            if !*running_for_thread.lock().unwrap() {
                                break true;
                            }
                        }
                        if let None = d.checked_sub(start.elapsed()) {
                            break false;
                        }
                    };
                    if terminate {
                        break;
                    }
                }
            }
            set_power(tail.unwrap_or(0));
        }));

        WorkoutHandle {
            join_handle,
            running,
        }
    }
}

#[allow(dead_code)]
// No repetitions, just set the final indefinite power
pub fn single_value(power: u16) -> Workout {
    Workout::new(CycleTree::Node((0, vec![])), Some(power))
}

#[allow(dead_code)]
// Hardcoded interval
pub fn interval_example() -> Workout {
    Workout::new(
        CycleTree::Node((
            1,
            vec![
                CycleTree::Leaf((Duration::from_secs(300), 80)),
                CycleTree::Node((
                    5,
                    vec![
                        CycleTree::Leaf((Duration::from_secs(180), 160)),
                        CycleTree::Leaf((Duration::from_secs(60), 80)),
                    ],
                )),
                CycleTree::Leaf((Duration::from_secs(300), 80)),
            ],
        )),
        None,
    )
}

#[allow(dead_code)]
// Standard intervals with one exception:  Go extra long on the first to try to
// hit a "steady state" for each interval as quickly as possible.
pub fn create_big_start_interval(
    warmup: (Duration, u16),
    count: usize,
    start: Duration,
    high: (Duration, u16),
    low: (Duration, u16),
    tail: Option<u16>,
) -> Workout {
    Workout::new(
        CycleTree::Node((
            1,
            vec![
                CycleTree::Leaf(warmup),
                CycleTree::Leaf((start, high.1)),
                CycleTree::Node((count - 1, vec![CycleTree::Leaf(low), CycleTree::Leaf(high)])),
            ],
        )),
        tail,
    )
}

#[allow(dead_code)]
// TODO: This could include Set/Add/Sub and then be 50 cycles of Add(15) each 30s
// Warm up for 5 minutes, then increase power by 15W every 30s until the subject
// must stop.
pub fn ramp_test(warmup_power: u16) -> Workout {
    let mut v = Vec::with_capacity(50);
    v.push(CycleTree::Leaf((Duration::from_secs(300), warmup_power)));
    let mut power = 100;
    for _ in 0..49 {
        v.push(CycleTree::Leaf((Duration::from_secs(30), warmup_power)));
        power = power + 15
    }
    Workout::new(CycleTree::Node((1, v)), None)
}
