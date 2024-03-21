use btleplug::api::{Central, CentralEvent, Peripheral, bleuuid::uuid_from_u16, WriteType};
use uuid::{Uuid, Builder};
use btleplug::Result;
use std::time::Duration;
use std::sync::Arc;
use tokio::sync::Mutex;
use futures::stream::StreamExt;

const UNLOCK_UUID: Uuid = Builder::from_bytes([
    0xA0, 0x26, 0xE0, 0x02, 0x0A, 0x7D, 0x4A, 0xB3, 0x97, 0xFA, 0xF1, 0x50, 0x0F, 0x9F, 0xEB, 0x8B,
]).into_uuid();

const TRAINER_UUID: Uuid = Builder::from_bytes([
    0xA0, 0x26, 0xE0, 0x05, 0x0A, 0x7D, 0x4A, 0xB3, 0x97, 0xFA, 0xF1, 0x50, 0x0F, 0x9F, 0xEB, 0x8B,
]).into_uuid();

pub const MEASURE_UUID: Uuid = uuid_from_u16(0x2A63);

pub const CONTROL_UUID: Uuid = Builder::from_bytes([
    0xA0, 0x26, 0xE0, 0x05, 0x0A, 0x7D, 0x4A, 0xB3, 0x97, 0xFA, 0xF1, 0x50, 0x0F, 0x9F, 0xEB, 0x8B,
]).into_uuid();

pub async fn connect<P: Peripheral, C: Central<Peripheral=P> + 'static>(central: &C) -> Result<Option<(P, Arc<Mutex<Option<u16>>>)>> {
    println!("Getting peripherals");
    let peripherals: Vec<P> = central.peripherals().await?;
    println!("Got peripherals list");
    let mut o_peripheral: Option<P> = None;
    for peripheral in peripherals {
        println!("Checking if device is kickr");
        let found_it = is_kickr(&peripheral).await?;
        if found_it {
          o_peripheral = Some(peripheral);
          break;
        }
    }

    match o_peripheral {
        None => Ok(None),
        Some(peripheral) => {
            println!("Found Kickr");

            peripheral.connect().await?;
            println!("Connected to Kickr");

            peripheral.discover_services().await?;
            println!("All characteristics discovered");

            first_time_setup(&peripheral).await?;

            let target_power = Arc::new(Mutex::new(None));

            let central_for_disconnects = central.clone();
            let tp_for_disconnects = target_power.clone();

            let mut events = central.events().await?;
            tokio::spawn(async move {
                while let Some(evt) = events.next().await {
                    if let CentralEvent::DeviceDisconnected(addr) = evt {
                        let p = central_for_disconnects.peripheral(&addr).await.unwrap();
                        if is_kickr(&p).await.unwrap() {
                            let wait = Duration::from_secs(10);
                            loop {
                                tokio::time::sleep(wait).await;
                                if p.connect().await.is_ok() {
                                    // TODO: Not sure what we could possibly do if these fail
                                    unlock(&p).await.unwrap();

                                    let guard = tp_for_disconnects.lock().await;
                                    if let Some(power) = *guard {
                                        write_power(&p, power).await.unwrap();
                                    }

                                    break;
                                }
                            }
                        }
                    }
                }
            });

            // TODO: This return type is pretty ugly, and means that users have
            // to wrangle these to independent pieces when it really should all
            // be encapsulated.  I couldn't figure out how to wrap
            // peripheral.notifications().  Also, long term, we probably just
            // want some sort of BleDeviceManager type that handles all devices
            // more abstractly (since that's part of the whole point of the
            // GATT interface), and worries about connecting to them and
            // maintaining connections where appropriate.  This is the bulk of
            // our code in the main function.
            Ok(Some((peripheral, target_power)))
        }
    }
}

async fn is_kickr(p: &impl Peripheral) -> Result<bool> {
    let op = p.properties().await?;
    Ok(match op {
      Some(properties) =>
        properties
            .local_name
            .iter()
            .any(|name| name.contains("KICKR")),
      None => false
    })
}

async fn first_time_setup(kickr: &impl Peripheral) -> Result<()> {
    let power_measurement = kickr
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == MEASURE_UUID)
        .unwrap();

    kickr.subscribe(&power_measurement).await?;
    println!("Subscribed to power measure");

    let trainer_characteristic = kickr
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == TRAINER_UUID)
        .unwrap();
    println!("Trainer char found.");

    kickr.subscribe(&trainer_characteristic).await?;
    println!("Subscribed to trainer characteristic");

    unlock(kickr).await?;
    Ok(())
}

async fn unlock(kickr: &impl Peripheral) -> Result<()> {
    let unlock_characteristic = kickr
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == UNLOCK_UUID)
        .unwrap();
    println!("Unlock char found.");

    kickr.write(
      &unlock_characteristic,
      &[0x20, 0xee, 0xfc],
      WriteType::WithoutResponse
    ).await?;
    println!("kickr unlocked!");
    Ok(())
}

pub async fn set_power(
    peripheral: &impl Peripheral,
    target_power_mutex: &Arc<Mutex<Option<u16>>>,
    power: u16,
) -> Result<()> {
    let mut tp_guard = target_power_mutex.lock().await;
    *tp_guard = Some(power);

     write_power(peripheral, power).await
}

async fn write_power(
    peripheral: &impl Peripheral,
    power: u16,
) -> Result<()> {
    let power_control_char = peripheral
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == CONTROL_UUID)
        // Kickr with a Control UUID is an invariant
        .unwrap();

    // TODO: Half the time (when using WithoutResponse) it seems this write
    // "gets stuck," and blocks anything else from happening.  If we use
    // WithResponse it blocks everything.
    // Monitoring dbus (via `sudo busctl monitor "org.bluez"`) makes it look like
    // everything is working.  We can see all the power events coming secondly,
    // and we can see both the request, response, and even the trainer char
    // send a notification of the change.
    peripheral.write(
        &power_control_char,
        &[0x42, (power & 0xff) as u8, ((power >> 8) & 0xff) as u8],
        WriteType::WithResponse
    ).await
}
