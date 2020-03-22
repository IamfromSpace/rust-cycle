use crate::ble::revolution_data::RevolutionData;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum AccumulatedTorqueSource {
    Wheel,
    Crank,
}

// A Struct that does not care about bit compression
#[derive(Debug, PartialEq, Clone)]
pub struct CyclingPowerMeasurement {
    pub instantaneous_power: i16,
    // Notably this is _truly_ a percent, not a rate
    // conversion to rate would be lossly
    pub pedal_power_balance_percent: Option<f32>,
    // Sum of the average torque measured per source rotation. Divide by
    // rotations to get average torque or multiply by 2pi to get total energy.
    // If you know the gearing you can translate from one source to the other.
    // Divide energy by source time to get average power.
    pub accumulated_torque: Option<(AccumulatedTorqueSource, f64)>,
    pub wheel_revolution_data: Option<RevolutionData>,
    pub crank_revolution_data: Option<RevolutionData>,
    // TODO: There are other fields, but they're all after these or in the flags
}

// Notably, this function always assumes a valid input
pub fn parse_cycling_power_measurement(data: &Vec<u8>) -> CyclingPowerMeasurement {
    let has_pedal_power_balance = data[0] & 1 == 1;
    let has_accumulated_torque = data[0] & 0b100 == 0b100;
    let has_wheel_data = data[0] & 0b10000 == 0b10000;
    let has_crank_data = data[0] & 0b100000 == 0b100000;
    let power_index = 2;
    let pedal_power_balance_index = 4;
    let accumulated_torque_index =
        pedal_power_balance_index + if has_pedal_power_balance { 1 } else { 0 };
    let wheel_data_index = accumulated_torque_index + if has_accumulated_torque { 2 } else { 0 };
    let crank_data_index = wheel_data_index + if has_wheel_data { 6 } else { 0 };

    CyclingPowerMeasurement {
        instantaneous_power: i16::from_le_bytes([data[power_index], data[power_index + 1]]),
        pedal_power_balance_percent: if has_pedal_power_balance {
            Some(data[pedal_power_balance_index] as f32 / 2.0)
        } else {
            None
        },
        accumulated_torque: if has_accumulated_torque {
            let source = if data[0] & 0b1000 == 0b1000 {
                AccumulatedTorqueSource::Crank
            } else {
                AccumulatedTorqueSource::Wheel
            };
            let torque = u16::from_le_bytes([
                data[accumulated_torque_index],
                data[accumulated_torque_index + 1],
            ]) as f64
                / 32.0;
            Some((source, torque))
        } else {
            None
        },
        // This isn't quite identical to CSC wheel data: it's /2048 instead of /1024
        wheel_revolution_data: if has_wheel_data {
            Some(RevolutionData {
                revolution_count: u32::from_le_bytes([
                    data[wheel_data_index],
                    data[wheel_data_index + 1],
                    data[wheel_data_index + 2],
                    data[wheel_data_index + 3],
                ]),
                last_revolution_event_time: (u16::from_le_bytes([
                    data[wheel_data_index + 4],
                    data[wheel_data_index + 4 + 1],
                ]) as f64)
                    / 2048.0,
            })
        } else {
            None
        },
        // This is identical to CSC crank data
        crank_revolution_data: if has_crank_data {
            Some(RevolutionData {
                revolution_count: u32::from_le_bytes([
                    data[crank_data_index],
                    data[crank_data_index + 1],
                    0,
                    0,
                ]),
                last_revolution_event_time: u16::from_le_bytes([
                    data[crank_data_index + 2],
                    data[crank_data_index + 2 + 1],
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
    use super::RevolutionData;

    use super::parse_cycling_power_measurement;
    use super::AccumulatedTorqueSource;
    use super::CyclingPowerMeasurement;

    #[test]
    fn parse_cpm_with_balance_torque_wheel_and_crank() {
        assert_eq!(
            CyclingPowerMeasurement {
                instantaneous_power: 0x0102,
                pedal_power_balance_percent: Some(49.5),
                accumulated_torque: Some((AccumulatedTorqueSource::Wheel, 0x0201 as f64 / 32.0)),
                wheel_revolution_data: Some(RevolutionData {
                    revolution_count: 0x04030201,
                    last_revolution_event_time: 0x0201 as f64 / 2048.0,
                }),
                crank_revolution_data: Some(RevolutionData {
                    revolution_count: 0x0201,
                    last_revolution_event_time: 0x0201 as f64 / 1024.0,
                }),
            },
            parse_cycling_power_measurement(&vec!(
                0b110101, 0, 2, 1, 99, 1, 2, 1, 2, 3, 4, 1, 2, 1, 2, 1, 2
            ))
        );
    }

    #[test]
    fn parse_cpm_with_accumulated_crank_torque() {
        assert_eq!(
            CyclingPowerMeasurement {
                instantaneous_power: 0x0102,
                pedal_power_balance_percent: None,
                accumulated_torque: Some((AccumulatedTorqueSource::Crank, 0x0201 as f64 / 32.0)),
                wheel_revolution_data: None,
                crank_revolution_data: Some(RevolutionData {
                    revolution_count: 0x0201,
                    last_revolution_event_time: 0x0201 as f64 / 1024.0,
                }),
            },
            parse_cycling_power_measurement(&vec!(0b101100, 0, 2, 1, 1, 2, 1, 2, 1, 2))
        );
    }

    #[test]
    fn parse_cpm_with_accumulated_wheel_torque() {
        assert_eq!(
            CyclingPowerMeasurement {
                instantaneous_power: 0x0102,
                pedal_power_balance_percent: None,
                accumulated_torque: Some((AccumulatedTorqueSource::Wheel, 0x0201 as f64 / 32.0)),
                wheel_revolution_data: Some(RevolutionData {
                    revolution_count: 0x04030201,
                    last_revolution_event_time: 0x0201 as f64 / 2048.0,
                }),
                crank_revolution_data: None,
            },
            parse_cycling_power_measurement(&vec!(0b10100, 0, 2, 1, 1, 2, 1, 2, 3, 4, 1, 2))
        );
    }

    #[test]
    fn parse_cpm_with_pedal_power_balance() {
        assert_eq!(
            CyclingPowerMeasurement {
                instantaneous_power: 0x0102,
                pedal_power_balance_percent: Some(49.5),
                accumulated_torque: None,
                wheel_revolution_data: None,
                crank_revolution_data: None,
            },
            parse_cycling_power_measurement(&vec!(1, 0, 2, 1, 99))
        );
    }

    #[test]
    fn parse_cpm_empty() {
        assert_eq!(
            CyclingPowerMeasurement {
                instantaneous_power: 0x0102,
                pedal_power_balance_percent: None,
                accumulated_torque: None,
                wheel_revolution_data: None,
                crank_revolution_data: None,
            },
            parse_cycling_power_measurement(&vec!(0, 0, 2, 1))
        );
    }
}
