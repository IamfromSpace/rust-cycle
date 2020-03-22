// A Struct that does not care about bit compression
#[derive(Debug, PartialEq, Clone)]
pub struct RevolutionData {
    // Total number of revolutions, this is years of data for wheels and cranks
    pub revolution_count: u32,
    // The time (in seconds) that the last revolution finished, this type is
    // chosen because it is both lossless and holds years of data.
    pub last_revolution_event_time: f64,
}
