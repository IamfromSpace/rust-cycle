use crate::ble::csc_measurement::MEASURE_UUID;
use btleplug::api::{Central, CentralEvent, NotificationHandler, Peripheral};
use btleplug::Result;
use std::{marker::PhantomData, thread, time::Duration};

pub struct Speed<C: Central<P>, P: Peripheral> {
    peripheral: P,
    central: PhantomData<C>,
}

impl<P: Peripheral, C: Central<P> + 'static> Speed<C, P> {
    pub fn new(central: C) -> Result<Option<Self>> {
        match central.peripherals().into_iter().find(is_speed) {
            Some(peripheral) => {
                println!("Found Speed Sensor");

                peripheral.connect()?;
                println!("Connected to Speed Sensor");

                peripheral.discover_characteristics()?;
                println!("All characteristics discovered");

                let speed_measurement = peripheral
                    .characteristics()
                    .into_iter()
                    .find(|c| c.uuid == MEASURE_UUID)
                    .unwrap();

                peripheral.subscribe(&speed_measurement).unwrap();
                println!("Subscribed to speed measure");

                // TODO: Is infinite delayed back-off retry really what we want here?  A couple
                // times may make sense, but possibly we should put the user in control of
                // how/when to try and reconnect.
                let central_for_disconnects = central.clone();
                central.on_event(Box::new(move |evt| {
                    if let CentralEvent::DeviceDisconnected(addr) = evt {
                        let p = central_for_disconnects.peripheral(addr).unwrap();
                        if is_speed(&p) {
                            let mut wait = 2;
                            loop {
                                thread::sleep(Duration::from_secs(wait));
                                if p.connect().is_ok() {
                                    break;
                                }
                                wait = wait * 2;
                            }
                        }
                    }
                }));

                Ok(Some(Speed {
                    peripheral,
                    central: PhantomData,
                }))
            }
            None => Ok(None),
        }
    }

    // TODO: Make this scoped just to Speed or just more specific in general?
    pub fn on_notification(&self, cb: NotificationHandler) {
        self.peripheral.on_notification(cb)
    }
}

impl<C: Central<P>, P: Peripheral> Drop for Speed<C, P> {
    fn drop(&mut self) {
        self.peripheral.clear_notification_handlers();
    }
}

fn is_speed(p: &impl Peripheral) -> bool {
    p.properties()
        .local_name
        .iter()
        .any(|name| name.contains("SPEED"))
}
