use btleplug::api::{Central, CentralEvent, NotificationHandler, Peripheral, UUID};
use btleplug::Result;
use std::{marker::PhantomData, thread, time::Duration};

pub const MEASURE_UUID: UUID = UUID::B16(0x2A63);

pub struct Assioma<C: Central<P>, P: Peripheral> {
    peripheral: P,
    central: PhantomData<C>,
}

impl<P: Peripheral, C: Central<P> + 'static> Assioma<C, P> {
    // TODO: It may make sense to use Type States to separate out new (Optional)
    // and connect (Result).  For this app, we really only care about
    // permanently connecting (but it would be nice to clean up connections on
    // exit).
    pub fn new(central: C) -> Result<Option<Self>> {
        match central.peripherals().into_iter().find(is_assioma) {
            None => Ok(None),
            Some(peripheral) => {
                peripheral.connect()?;
                println!("Connected to Assioma");

                let mut power_measurement = None;
                let mut wait = None;

                // TODO: These retries _never_ seem to work, so they actually
                // make the problem worse, by drawing out the time it takes for
                // the application to crash, preventing a human retry from the
                // beginning.
                while power_measurement.is_none() {
                    wait.iter().for_each(|&t| {
                        if t > 10 {
                            panic!(
                                "Could not find the power measurement characteristic after retries"
                            );
                        } else {
                            thread::sleep(Duration::from_secs(t));
                            println!("Couldn't find the power measurement characteristic on discovery.  Trying again...");
                        }
                    });

                    peripheral.discover_characteristics()?;
                    println!("All characteristics discovered");

                    power_measurement = peripheral
                        .characteristics()
                        .into_iter()
                        .find(|c| c.uuid == MEASURE_UUID);

                    wait = Some(wait.unwrap_or(1) * 2);
                }

                let power_measurement = power_measurement.unwrap();

                peripheral.subscribe(&power_measurement)?;
                println!("Subscribed to power measure");

                let central_for_disconnects = central.clone();
                central.on_event(Box::new(move |evt| {
                    if let CentralEvent::DeviceDisconnected(addr) = evt {
                        let p = central_for_disconnects.peripheral(addr).unwrap();
                        if is_assioma(&p) {
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

                Ok(Some(Assioma {
                    peripheral,
                    central: PhantomData,
                }))
            }
        }
    }

    // TODO: Make this scoped just to power or just more specific in general?
    pub fn on_notification(&self, cb: NotificationHandler) {
        self.peripheral.on_notification(cb)
    }
}

impl<C: Central<P>, P: Peripheral> Drop for Assioma<C, P> {
    fn drop(&mut self) {
        self.peripheral.clear_notification_handlers();
    }
}

fn is_assioma(p: &impl Peripheral) -> bool {
    p.properties()
        .local_name
        .iter()
        .any(|name| name.contains("ASSIOMA"))
}
