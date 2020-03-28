use btleplug::api::{Central, CentralEvent, NotificationHandler, Peripheral, UUID};
use btleplug::Result;
use std::{marker::PhantomData, thread, time::Duration};

pub const MEASURE_UUID: UUID = UUID::B16(0x2A5B);

pub struct Cadence<C: Central<P>, P: Peripheral> {
    peripheral: P,
    central: PhantomData<C>,
}

impl<P: Peripheral, C: Central<P> + 'static> Cadence<C, P> {
    pub fn new(central: C) -> Result<Option<Self>> {
        match central.peripherals().into_iter().find(is_cadence) {
            Some(peripheral) => {
                println!("Found Cadence");

                peripheral.connect()?;
                println!("Connected to Cadence");

                peripheral.discover_characteristics()?;
                println!("All characteristics discovered");

                let cadence_measurement = peripheral
                    .characteristics()
                    .into_iter()
                    .find(|c| c.uuid == MEASURE_UUID)
                    .unwrap();

                peripheral.subscribe(&cadence_measurement).unwrap();
                println!("Subscribed to cadence measure");

                let central_for_disconnects = central.clone();
                central.on_event(Box::new(move |evt| {
                    if let CentralEvent::DeviceDisconnected(addr) = evt {
                        let p = central_for_disconnects.peripheral(addr).unwrap();
                        if is_cadence(&p) {
                            thread::sleep(Duration::from_secs(2));
                            p.connect().unwrap();
                        }
                    }
                }));

                Ok(Some(Cadence {
                    peripheral,
                    central: PhantomData,
                }))
            }
            None => Ok(None),
        }
    }

    // TODO: Make this scoped just to Cadence or just more specific in general?
    pub fn on_notification(&self, cb: NotificationHandler) {
        self.peripheral.on_notification(cb)
    }
}

impl<C: Central<P>, P: Peripheral> Drop for Cadence<C, P> {
    fn drop(&mut self) {
        self.peripheral.clear_notification_handlers();
    }
}

fn is_cadence(p: &impl Peripheral) -> bool {
    p.properties()
        .local_name
        .iter()
        .any(|name| name.contains("CADENCE"))
}
