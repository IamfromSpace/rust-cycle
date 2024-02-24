use crate::ble::revolution_data;
use crate::ble::revolution_data::RevolutionData;
use uuid::Uuid;
use btleplug::api::bleuuid::uuid_from_u16;

pub const MEASURE_UUID: Uuid = uuid_from_u16(0x2A5B);

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

pub fn checked_wheel_rpm_and_new_count(
    a: Option<&CscMeasurement>,
    b: &CscMeasurement,
) -> Option<(f64, u32)> {
    // If we don't have previous measurement, then continue, but if we have a previous, but it
    // doesn't have wheel data, then we abort.
    let a = crate::utils::sequence_option_option(a.map(|x| x.wheel.as_ref()));
    let b = b.wheel.as_ref();
    crate::utils::lift_a2_option(a, b, revolution_data::checked_wheel_rpm_and_new_count)
        .and_then(|x| x)
}

pub fn checked_crank_rpm_and_new_count(
    a: Option<&CscMeasurement>,
    b: &CscMeasurement,
) -> Option<(f64, u32)> {
    // If we don't have previous measurement, then continue, but if we have a previous, but it
    // doesn't have crank data, then we abort.
    let a = crate::utils::sequence_option_option(a.map(|x| x.crank.as_ref()));
    let b = b.crank.as_ref();
    crate::utils::lift_a2_option(a, b, revolution_data::checked_crank_rpm_and_new_count)
        .and_then(|x| x)
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

    use super::checked_crank_rpm_and_new_count;
    #[test]
    fn overflow_works() {
        assert_eq!(
            Some((95.10835913312694, 2)),
            checked_crank_rpm_and_new_count(
                Some(&CscMeasurement {
                    wheel: None,
                    crank: Some(RevolutionData {
                        revolution_count: 4434,
                        last_revolution_event_time: 62.9365234375
                    })
                }),
                &CscMeasurement {
                    wheel: None,
                    crank: Some(RevolutionData {
                        revolution_count: 4436,
                        last_revolution_event_time: 0.1982421875
                    })
                }
            )
        )
    }

    #[test]
    fn does_not_default_if_crank_data_is_not_present() {
        assert_eq!(
            None,
            checked_crank_rpm_and_new_count(
                Some(&CscMeasurement {
                    wheel: None,
                    crank: None
                }),
                &CscMeasurement {
                    wheel: None,
                    crank: Some(RevolutionData {
                        revolution_count: 4436,
                        last_revolution_event_time: 0.1982421875
                    })
                }
            )
        )
    }

    #[test]
    fn defaults_if_missing_measurement_entirely() {
        assert_eq!(
            Some((80.0, 2)),
            checked_crank_rpm_and_new_count(
                None,
                &CscMeasurement {
                    wheel: None,
                    crank: Some(RevolutionData {
                        revolution_count: 2,
                        last_revolution_event_time: 1.5
                    })
                }
            )
        )
    }

    #[test]
    fn none_if_not_new() {
        assert_eq!(
            None,
            checked_crank_rpm_and_new_count(
                Some(&CscMeasurement {
                    wheel: None,
                    crank: Some(RevolutionData {
                        revolution_count: 2,
                        last_revolution_event_time: 1.5
                    })
                }),
                &CscMeasurement {
                    wheel: None,
                    crank: Some(RevolutionData {
                        revolution_count: 2,
                        last_revolution_event_time: 1.5
                    })
                }
            )
        )
    }

    #[test]
    fn crank_does_not_report_impossible_numbers() {
        assert_eq!(
            None,
            checked_crank_rpm_and_new_count(
                Some(&CscMeasurement {
                    wheel: None,
                    crank: Some(RevolutionData {
                        revolution_count: 4434,
                        last_revolution_event_time: 1.0
                    }),
                }),
                &CscMeasurement {
                    wheel: None,
                    crank: Some(RevolutionData {
                        revolution_count: 4439,
                        last_revolution_event_time: 2.0
                    }),
                }
            )
        )
    }

    use super::checked_wheel_rpm_and_new_count;
    #[test]
    fn wheel_overflow_works() {
        assert_eq!(
            Some((95.10835913312694, 2)),
            checked_wheel_rpm_and_new_count(
                Some(&CscMeasurement {
                    wheel: Some(RevolutionData {
                        revolution_count: 4434,
                        last_revolution_event_time: 62.9365234375
                    }),
                    crank: None,
                }),
                &CscMeasurement {
                    wheel: Some(RevolutionData {
                        revolution_count: 4436,
                        last_revolution_event_time: 0.1982421875
                    }),
                    crank: None,
                }
            )
        )
    }

    #[test]
    fn wheel_does_not_default_if_crank_data_is_not_present() {
        assert_eq!(
            None,
            checked_wheel_rpm_and_new_count(
                Some(&CscMeasurement {
                    wheel: None,
                    crank: None
                }),
                &CscMeasurement {
                    wheel: Some(RevolutionData {
                        revolution_count: 4436,
                        last_revolution_event_time: 0.1982421875
                    }),
                    crank: None,
                }
            )
        )
    }

    #[test]
    fn wheel_defaults_if_missing_measurement_entirely() {
        assert_eq!(
            Some((80.0, 2)),
            checked_wheel_rpm_and_new_count(
                None,
                &CscMeasurement {
                    wheel: Some(RevolutionData {
                        revolution_count: 2,
                        last_revolution_event_time: 1.5
                    }),
                    crank: None,
                }
            )
        )
    }

    #[test]
    fn wheel_assumes_missing_when_backwards() {
        assert_eq!(
            Some((80.0, 2)),
            checked_wheel_rpm_and_new_count(
                Some(&CscMeasurement {
                    wheel: Some(RevolutionData {
                        revolution_count: 4434,
                        last_revolution_event_time: 62.9365234375
                    }),
                    crank: None,
                }),
                &CscMeasurement {
                    wheel: Some(RevolutionData {
                        revolution_count: 2,
                        last_revolution_event_time: 1.5
                    }),
                    crank: None,
                }
            )
        )
    }

    #[test]
    fn wheel_does_not_report_impossible_numbers() {
        assert_eq!(
            None,
            checked_wheel_rpm_and_new_count(
                Some(&CscMeasurement {
                    wheel: Some(RevolutionData {
                        revolution_count: 4434,
                        last_revolution_event_time: 1.0
                    }),
                    crank: None,
                }),
                &CscMeasurement {
                    wheel: Some(RevolutionData {
                        revolution_count: 4455,
                        last_revolution_event_time: 2.0
                    }),
                    crank: None,
                }
            )
        )
    }
}
