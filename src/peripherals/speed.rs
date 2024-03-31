use crate::ble::csc_measurement::MEASURE_UUID;
use btleplug::api::{Central, CentralEvent, Peripheral};
use btleplug::Result;
use std::time::Duration;
use futures::stream::StreamExt;

pub async fn connect<P: Peripheral, C: Central<Peripheral=P> + 'static>(central: &C) -> Result<Option<P>> {
    println!("Getting peripherals");
    let peripherals: Vec<P> = central.peripherals().await?;
    println!("Got peripherals list");
    let mut o_peripheral: Option<P> = None;
    for peripheral in peripherals {
        println!("Checking if device is speed sensor");
        let found_it = is_speed(&peripheral).await?;
        if found_it {
          o_peripheral = Some(peripheral);
          break;
        }
    }

    match o_peripheral {
        Some(peripheral) => {
            println!("Found Speed Sensor");

            peripheral.connect().await?;
            println!("Connected to Speed Sensor");

            peripheral.discover_services().await?;
            println!("All characteristics discovered");

            let o_speed_measurement = peripheral
                .characteristics()
                .into_iter()
                .find(|c| c.uuid == MEASURE_UUID);

            match o_speed_measurement {
                None => {
                    peripheral.disconnect().await?;
                    Ok(None)
                },
                Some(speed_measurement) => {
                    peripheral.subscribe(&speed_measurement).await?;
                    println!("Subscribed to speed measure");

                    let central_for_disconnects = central.clone();
                    let mut events = central.events().await?;
                    tokio::spawn(async move {
                        while let Some(evt) = events.next().await {
                            if let CentralEvent::DeviceDisconnected(addr) = evt {
                                println!("Speed Disconnected.");
                                let p = central_for_disconnects.peripheral(&addr).await.unwrap();
                                if is_speed(&p).await.unwrap() {
                                    let wait = Duration::from_secs(10);
                                    loop {
                                        tokio::time::sleep(wait).await;
                                        println!("Attempting Speed reconnect.");
                                        if p.connect().await.is_ok() {
                                            println!("Speed reconnected.");
                                            break;
                                        }
                                        println!("Speed reconnect failed.");
                                    }
                                }
                            }
                        };
                    });

                    Ok(Some(peripheral))
                }
            }
        }
        None => Ok(None),
    }
}

async fn is_speed(p: &impl Peripheral) -> Result<bool> {
    let op = p.properties().await?;
    Ok(match op {
      Some(properties) =>
        properties
            .local_name
            .iter()
            .any(|name| name.contains("SPEED")),
      None => false
    })
}
