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

#[derive(Clone)]
pub struct WorkoutState {
    running: bool,
    offset: i16,
}

pub struct WorkoutHandle {
    pub state: Arc<Mutex<WorkoutState>>,
    join_handle: Option<JoinHandle<()>>,
}

impl WorkoutHandle {
    pub fn exit(&mut self) {
        {
            let mut state = self.state.lock().unwrap();
            state.running = false;
        }
        if let Some(jh) = mem::replace(&mut self.join_handle, None) {
            jh.join().unwrap();
        }
    }
}

// TODO: This helper is only here because it's clunky to access the state,
// because WorkoutHandle can't be clone (because of JoinHandle).
pub fn add_offset(state: &Arc<Mutex<WorkoutState>>, offset: i16) {
    {
        let mut state = state.lock().unwrap();
        state.offset += offset;
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
        let state = Arc::new(Mutex::new(WorkoutState {
            running: true,
            offset: 0,
        }));
        let state_for_thread = state.clone();
        let Workout { ct, tail } = self;
        let tail_iter = tail.map(|x| (Duration::from_secs(1000000), x)).into_iter();
        let join_handle = Some(thread::spawn(move || {
            let mut d = Duration::from_secs(0);
            let mut last_offset: i16 = 0;

            for (wait, power) in ct.into_iter().chain(tail_iter) {
                // Overflow is not a consideration for the timeline of a single workout
                d = d.checked_add(wait).unwrap();
                let e = start.elapsed();
                // If duration is negative, we continue on.
                if let Some(_) = d.checked_sub(e) {
                    set_power(((power as i16) + last_offset) as u16);

                    // We loop and check every 50ms if we should move to the
                    // next power or if the workout is teriminated.
                    let terminate = loop {
                        thread::sleep(Duration::from_millis(50));
                        {
                            let state = state_for_thread.lock().unwrap();
                            if !state.running {
                                break true;
                            }

                            // Check to see if the offset has changed, if so,
                            // record the new offset and immediately update the
                            // power.
                            if state.offset != last_offset {
                                last_offset = state.offset;
                                set_power(((power as i16) + last_offset) as u16);
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
        }));

        WorkoutHandle { join_handle, state }
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
