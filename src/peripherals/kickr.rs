use btleplug::api::{Central, CentralEvent, Characteristic, NotificationHandler, Peripheral, UUID};
use btleplug::Result;
use std::{
    marker::PhantomData,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

const UNLOCK_UUID: UUID = UUID::B128([
    0x8B, 0xEB, 0x9F, 0x0F, 0x50, 0xF1, 0xFA, 0x97, 0xB3, 0x4A, 0x7D, 0x0A, 0x02, 0xE0, 0x26, 0xA0,
]);

const TRAINER_UUID: UUID = UUID::B128([
    0x8B, 0xEB, 0x9F, 0x0F, 0x50, 0xF1, 0xFA, 0x97, 0xB3, 0x4A, 0x7D, 0x0A, 0x05, 0xE0, 0x26, 0xA0,
]);

pub const MEASURE_UUID: UUID = UUID::B16(0x2A63);

pub const CONTROL_UUID: UUID = UUID::B128([
    0x8B, 0xEB, 0x9F, 0x0F, 0x50, 0xF1, 0xFA, 0x97, 0xB3, 0x4A, 0x7D, 0x0A, 0x05, 0xE0, 0x26, 0xA0,
]);

pub struct Kickr<C: Central<P>, P: Peripheral> {
    peripheral: P,
    power_control_char: Characteristic,
    target_power: Arc<Mutex<Option<u16>>>,
    central: PhantomData<C>,
}

impl<P: Peripheral, C: Central<P> + 'static> Kickr<C, P> {
    // TODO: It may make sense to use Type States to separate out new (Optional)
    // and connect (Result).  For this app, we really only care about
    // permanently connecting (but it would be nice to clean up connections on
    // exit).
    pub fn new(central: C) -> Result<Option<Self>> {
        match central.peripherals().into_iter().find(is_kickr) {
            None => Ok(None),
            Some(peripheral) => {
                peripheral.connect()?;
                println!("Connected to KICKR");

                peripheral.discover_characteristics()?;
                println!("All characteristics discovered");

                first_time_setup(&peripheral)?;
                unlock(&peripheral)?;

                let power_control_char = peripheral
                    .characteristics()
                    .into_iter()
                    .find(|c| c.uuid == CONTROL_UUID)
                    // Kickr with a Control UUID is an invariant
                    .unwrap();

                let target_power = Arc::new(Mutex::new(None));

                let central_for_disconnects = central.clone();
                let tp_for_disconnects = target_power.clone();
                let pcc_for_disconnects = power_control_char.clone();

                central.on_event(Box::new(move |evt| {
                    if let CentralEvent::DeviceDisconnected(addr) = evt {
                        let p = central_for_disconnects.peripheral(addr).unwrap();
                        if is_kickr(&p) {
                            let mut wait = 2;
                            loop {
                                thread::sleep(Duration::from_secs(wait));
                                if p.connect().is_ok() {
                                    // TODO: Not sure what we could possibly do if these fail
                                    unlock(&p).unwrap();
                                    if let Some(power) = *(tp_for_disconnects.lock().unwrap()) {
                                        set_power(&p, &pcc_for_disconnects, power).unwrap();
                                    }
                                    break;
                                }
                                wait = wait * 2;
                            }
                        }
                    }
                }));

                Ok(Some(Kickr {
                    peripheral,
                    power_control_char,
                    target_power,
                    central: PhantomData,
                }))
            }
        }
    }

    pub fn set_power(&self, power: u16) -> Result<()> {
        // TODO: We get a notification when this is done, so if we hold the lock
        // until then, we can use eventing to ensure a good sync.
        let mut tp_guard = self.target_power.lock().unwrap();
        *tp_guard = Some(power);

        set_power(&self.peripheral, &self.power_control_char, power)
    }

    // TODO: Make this scoped just to power or just more specific in general?
    pub fn on_notification(&self, cb: NotificationHandler) {
        self.peripheral.on_notification(cb)
    }
}

impl<C: Central<P>, P: Peripheral> Drop for Kickr<C, P> {
    fn drop(&mut self) {
        self.peripheral.clear_notification_handlers();
    }
}

fn is_kickr(p: &impl Peripheral) -> bool {
    p.properties()
        .local_name
        .iter()
        .any(|name| name.contains("KICKR"))
}

fn first_time_setup(kickr: &impl Peripheral) -> Result<()> {
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

fn unlock(kickr: &impl Peripheral) -> Result<()> {
    let unlock_characteristic = kickr
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == UNLOCK_UUID)
        .unwrap();
    println!("Unlock char found.");

    kickr.command(&unlock_characteristic, &[0x20, 0xee, 0xfc])?;
    println!("kickr unlocked!");
    Ok(())
}

fn set_power(
    peripheral: &impl Peripheral,
    power_control_char: &Characteristic,
    power: u16,
) -> Result<()> {
    peripheral.request(
        power_control_char,
        &[0x42, (power & 0xff) as u8, ((power >> 8) & 0xff) as u8],
    )?;
    thread::sleep(Duration::from_secs(1));
    Ok(())
}
