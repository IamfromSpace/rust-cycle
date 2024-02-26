use btleplug::api::{Central, CentralEvent, Peripheral, ValueNotification, bleuuid::uuid_from_u16};
use uuid::Uuid;
use btleplug::Result;
use std::{marker::PhantomData, time::Duration};
use std::pin::Pin;
use std::future::Future;
use futures::stream::StreamExt;
use futures_core::stream::Stream;

pub const MEASURE_UUID: Uuid = uuid_from_u16(0x2A63);

pub async fn connect<P: Peripheral, C: Central<Peripheral=P> + 'static>(central: &C) -> Result<Option<P>> {
    // TODO: It may make sense to use Type States to separate out new (Optional)
    // and connect (Result).  For this app, we really only care about
    // permanently connecting (but it would be nice to clean up connections on
    // exit).
    println!("Getting peripherals");
    let peripherals: Vec<P> = central.peripherals().await?;
    println!("Got peripherals list");
    let mut o_peripheral: Option<P> = None;
    for peripheral in peripherals {
        println!("Checking if device is assioma");
        let found_it = is_assioma(&peripheral).await?;
        if found_it {
          o_peripheral = Some(peripheral);
          break;
        }
    }

    match o_peripheral {
        None => Ok(None),
        Some(peripheral) => {
            peripheral.connect().await?;
            println!("Connected to Assioma");

            peripheral.discover_services().await?;
            println!("All characteristics discovered");

            let o_power_measurement = peripheral
                .characteristics()
                .into_iter()
                .find(|c| c.uuid == MEASURE_UUID);

            match o_power_measurement {
                None => {
                    peripheral.disconnect().await?;
                    Ok(None)
                },
                Some(power_measurement) => {
                    peripheral.subscribe(&power_measurement).await?;
                    println!("Subscribed to power measure");

                    let central_for_disconnects = central.clone();
                    let mut events = central.events().await?;
                    tokio::spawn(async move {
                        while let Some(evt) = events.next().await {
                            if let CentralEvent::DeviceDisconnected(addr) = evt {
                                let p = central_for_disconnects.peripheral(&addr).await.unwrap();
                                if is_assioma(&p).await.unwrap() {
                                    let mut wait = 2;
                                    loop {
                                        tokio::time::sleep(Duration::from_secs(wait)).await;
                                        if p.connect().await.is_ok() {
                                            break;
                                        }
                                        wait = wait * 2;
                                    }
                                }
                            }
                        };
                    });

                    Ok(Some(peripheral))
                }
            }

        }
    }
}

async fn is_assioma(p: &impl Peripheral) -> Result<bool> {
    let op = p.properties().await?;
    Ok(match op {
      Some(properties) =>
        properties
            .local_name
            .iter()
            .any(|name| name.contains("ASSIOMA")),
      None => false
    })
}
