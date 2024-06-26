use btleplug::api::{Central, CentralEvent, Peripheral, bleuuid::uuid_from_u16};
use uuid::Uuid;
use btleplug::Result;
use std::time::Duration;
use futures::stream::StreamExt;

pub const MEASURE_UUID: Uuid = uuid_from_u16(0x2A37);

pub async fn connect<P: Peripheral, C: Central<Peripheral=P> + 'static>(central: &C) -> Result<Option<P>> {
    println!("Getting peripherals");
    let peripherals: Vec<P> = central.peripherals().await?;
    println!("Got peripherals list");
    let mut o_peripheral: Option<P> = None;
    for peripheral in peripherals {
        println!("Checking if device is hrm");
        let found_it = is_hrm(&peripheral).await?;
        if found_it {
          o_peripheral = Some(peripheral);
          break;
        }
    }

    match o_peripheral {
        Some(peripheral) => {
            println!("Found HRM");

            peripheral.connect().await?;
            println!("Connected to HRM");

            peripheral.discover_services().await?;
            println!("All characteristics discovered");

            let o_hr_measurement = peripheral
                .characteristics()
                .into_iter()
                .find(|c| c.uuid == MEASURE_UUID);

            match o_hr_measurement {
                None => {
                    peripheral.disconnect().await?;
                    Ok(None)
                },
                Some(hr_measurement) => {
                    peripheral.subscribe(&hr_measurement).await?;
                    println!("Subscribed to hr measure");

                    let central_for_disconnects = central.clone();
                    let mut events = central.events().await?;
                    tokio::spawn(async move {
                        while let Some(evt) = events.next().await {
                            if let CentralEvent::DeviceDisconnected(addr) = evt {
                                println!("HRM Disconnected.");
                                let p = central_for_disconnects.peripheral(&addr).await.unwrap();
                                if is_hrm(&p).await.unwrap() {
                                    let wait = Duration::from_secs(10);
                                    loop {
                                        tokio::time::sleep(wait).await;
                                        println!("Attempting HRM reconnect.");
                                        if p.connect().await.is_ok() {
                                            println!("HRM reconnected.");
                                            break;
                                        }
                                        println!("HRM reconnect failed.");
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

async fn is_hrm(p: &impl Peripheral) -> Result<bool> {
    let op = p.properties().await?;
    Ok(match op {
      Some(properties) =>
        properties
            .local_name
            .iter()
            .any(|name| name.contains("Polar")),
      None => false
    })
}
