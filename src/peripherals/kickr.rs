use btleplug::api::{Peripheral, UUID};
use btleplug::Result;

// TODO: This should really return a whole new Kickr struct that encapsulates
// all ble details and just has methods that apply to Kickrs.

const UNLOCK_UUID: UUID = UUID::B128([
    0x8B, 0xEB, 0x9F, 0x0F, 0x50, 0xF1, 0xFA, 0x97, 0xB3, 0x4A, 0x7D, 0x0A, 0x02, 0xE0, 0x26, 0xA0,
]);

const TRAINER_UUID: UUID = UUID::B128([
    0x8B, 0xEB, 0x9F, 0x0F, 0x50, 0xF1, 0xFA, 0x97, 0xB3, 0x4A, 0x7D, 0x0A, 0x05, 0xE0, 0x26, 0xA0,
]);

const MEASURE_UUID: UUID = UUID::B16(0x2A63);

const CONTROL_UUID: UUID = UUID::B128([
    0x8B, 0xEB, 0x9F, 0x0F, 0x50, 0xF1, 0xFA, 0x97, 0xB3, 0x4A, 0x7D, 0x0A, 0x05, 0xE0, 0x26, 0xA0,
]);

pub fn is_kickr(p: &impl Peripheral) -> bool {
    p.properties()
        .local_name
        .iter()
        .any(|name| name.contains("KICKR"))
}

pub fn setup(kickr: &impl Peripheral) -> Result<()> {
    kickr.connect().unwrap();
    println!("Connected to KICKR");

    kickr.discover_characteristics().unwrap();
    println!("All characteristics discovered");

    let power_measurement = kickr
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == MEASURE_UUID)
        .unwrap();

    kickr.subscribe(&power_measurement)?;
    println!("Subscribed to power measure");

    let trainer_characteristic = kickr
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == TRAINER_UUID)
        .unwrap();
    println!("Trainer char found.");

    kickr.subscribe(&trainer_characteristic)?;
    println!("Subscribed to trainer characteristic");
    Ok(())
}

pub fn on_connect(kickr: &impl Peripheral) -> Result<impl Fn(u8) -> Result<()>> {
    let unlock_characteristic = kickr
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == UNLOCK_UUID)
        .unwrap();
    println!("Unlock char found.");

    kickr.command(&unlock_characteristic, &[0x20, 0xee, 0xfc])?;
    println!("kickr unlocked!");

    let power_control = kickr
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == CONTROL_UUID)
        .unwrap();

    let k = kickr.clone();
    let pc = power_control.clone();

    // TODO: This whole thing needs some thinking.
    Ok(move |power| k.command(&pc, &[0x42, power, 0]))
}
