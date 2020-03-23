use crate::cycle_tree::CycleTree;
use std::{
    thread,
    time::{Duration, Instant},
};

// A workout is made up a cycle tree that holds how long a certain amount of
// power should be held for, and then optionally a final power that is held
// indefinitely at the end of the workout (defaults to 0).
pub type Workout = (CycleTree<(Duration, u16)>, Option<u16>);

// *** Note that this will completely hi-jack your thread! ***
// This also eventually self-corrects any drift, because we always target the
// correct total time for our changes.
pub fn run_workout<F: Fn(u16)>(start: Instant, workout: Workout, set_power: F) {
    let mut d = Duration::from_secs(0);
    for (wait, power) in workout.0.into_iter() {
        // Overflow is not a consideration for the timeline of a single workout
        d = d.checked_add(wait).unwrap();
        let e = start.elapsed();
        // If duration is negative, we continue on.
        if let Some(remaining) = d.checked_sub(e) {
            set_power(power);
            thread::sleep(remaining);
        }
    }
    set_power(workout.1.unwrap_or(0));
}

#[allow(dead_code)]
// No repetitions, just set the final indefinite power
pub fn single_value(power: u16) -> Workout {
    (CycleTree::Node((0, vec![])), Some(power))
}

#[allow(dead_code)]
// Hardcoded interval
pub fn interval_example() -> Workout {
    (
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
    warmup: (u64, u16),
    count: usize,
    start: u64,
    high: (u64, u16),
    low: (u64, u16),
    cooldown: (u64, u16),
) -> Workout {
    (
        CycleTree::Node((
            1,
            vec![
                CycleTree::Leaf((Duration::from_secs(warmup.0), warmup.1)),
                CycleTree::Leaf((Duration::from_secs(start), high.1)),
                CycleTree::Node((
                    count,
                    vec![
                        CycleTree::Leaf((Duration::from_secs(low.0), low.1)),
                        CycleTree::Leaf((Duration::from_secs(high.0), high.1)),
                    ],
                )),
                CycleTree::Leaf((Duration::from_secs(cooldown.0), cooldown.1)),
            ],
        )),
        None,
    )
}
