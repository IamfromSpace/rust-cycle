use crate::ble::revolution_data::RevolutionData;

// A Struct that does not care about bit compression
#[derive(Debug, PartialEq, Clone)]
pub struct CscMeasurement {
    // Data about wheel rotations
    pub wheel: Option<RevolutionData>,
    // Data about crank rotations
    pub crank: Option<RevolutionData>,
}

// Notably, this function always assumes a valid input
pub fn parse_csc_measurement(data: &Vec<u8>) -> CscMeasurement {
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

#[cfg(test)]
mod tests {
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
}
