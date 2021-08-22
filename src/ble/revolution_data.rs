// A Struct that does not care about bit compression
#[derive(Debug, PartialEq, Clone)]
pub struct RevolutionData {
    // Total number of revolutions, this is years of data for wheels and cranks
    pub revolution_count: u32,
    // The time (in seconds) that the last revolution finished, this type is
    // chosen because it is both lossless and holds years of data.
    pub last_revolution_event_time: f64,
}

// TODO: How to better handle overflow when managing raw/decoded data
pub fn checked_crank_rpm_and_new_count(
    a: Option<&RevolutionData>,
    b: &RevolutionData,
) -> Option<(f64, u32)> {
    let duration = checked_duration(a, b);
    if duration == 0.0 {
        None
    } else {
        let a_revolution_count = a.map_or(0, |x| x.revolution_count);
        let new_revolutions = if b.revolution_count > a_revolution_count {
            b.revolution_count - a_revolution_count
        } else {
            0x10000 + b.revolution_count - a_revolution_count
        };

        let rpm = new_revolutions as f64 * 60.0 / duration;

        // This appears to be a world record, and videos of 260 are completely insane.
        // TODO: is it worth still reporting new_revolutions?
        if rpm > 271.0 {
            None
        } else {
            Some((rpm, new_revolutions))
        }
    }
}

pub fn checked_wheel_rpm_and_new_count(
    a: Option<&RevolutionData>,
    b: &RevolutionData,
) -> Option<(f64, u32)> {
    let duration = checked_duration(a, b);
    if duration == 0.0 {
        None
    } else {
        let a_revolution_count = a.map_or(0, |x| x.revolution_count);
        // It's not really feasible for wheels to overflow, this is in the billions of meters.
        if b.revolution_count > a_revolution_count {
            let new_revolutions = b.revolution_count - a_revolution_count;

            let rpm = new_revolutions as f64 * 60.0 / duration;

            // This is just above the current world record from the AeroVelo Eta at an absolutely
            // mind-boggling 144kmh (with thin 650c tires).
            // TODO: is it worth still reporting new_revolutions?
            if rpm > 1250.0 {
                None
            } else {
                Some((rpm, new_revolutions))
            }
        } else {
            // This indicates a reset, so we instead assume the two events are not connected and
            // there is no previous.
            checked_wheel_rpm_and_new_count(None, b)
        }
    }
}

pub fn checked_duration(a: Option<&RevolutionData>, b: &RevolutionData) -> f64 {
    let a_last_revolution_event_time = a.map_or(0.0, |x| x.last_revolution_event_time);
    if b.last_revolution_event_time >= a_last_revolution_event_time {
        b.last_revolution_event_time - a_last_revolution_event_time
    } else {
        0b1000000 as f64 + b.last_revolution_event_time - a_last_revolution_event_time
    }
}
