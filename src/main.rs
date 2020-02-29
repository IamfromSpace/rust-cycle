extern crate btleplug;

use std::io::{stdout,Write};
use std::thread;
use std::time::Duration;
use btleplug::bluez::manager::Manager;
use btleplug::api::{UUID, Central, Peripheral};

pub fn main() {
    println!("Getting Manager...");
    stdout().flush().unwrap();

    let manager = Manager::new().unwrap();

    let mut adapter = manager.adapters().unwrap().into_iter().next().unwrap();

    adapter = manager.down(&adapter).unwrap();
    adapter = manager.up(&adapter).unwrap();

    let central = adapter.connect().unwrap();

    println!("Starting Scan...");
    stdout().flush().unwrap();
    central.start_scan().unwrap();

    thread::sleep(Duration::from_secs(5));

    println!("Stopping scan...");
    stdout().flush().unwrap();
    central.stop_scan().unwrap();

    println!("{:?}", central.peripherals());

    let kickr = central.peripherals().into_iter()
        .find(|p| p.properties().local_name.iter()
              .any(|name| name.contains("KICKR"))).unwrap();
    println!("Found KICKR");
    stdout().flush().unwrap();

    kickr.connect().unwrap();
    println!("Connected to KICKR");
    stdout().flush().unwrap();

    kickr.discover_characteristics().unwrap();
    println!("All characteristics discovered");
    stdout().flush().unwrap();

    println!("{:?}", kickr.characteristics());
    let power_measurement = kickr.characteristics().into_iter().find(|c| c.uuid == UUID::B16(0x2A63)).unwrap();

    kickr.subscribe(&power_measurement).unwrap();
    println!("Subscribed to power measure");
    stdout().flush().unwrap();

    kickr.on_notification(Box::new(|n| {
        println!("{:?}", n);
        stdout().flush().unwrap();
    }));
    
    loop {}
}

#[cfg(test)]
mod tests {

    #[test]
    fn my_test() {
        assert_eq!(true, true);
    }
}
