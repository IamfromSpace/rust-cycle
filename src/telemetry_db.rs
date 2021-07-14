use btleplug::api::UUID;
use nmea0183::ParseResult;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::time::Duration;

#[derive(Clone)]
pub struct TelemetryDb {
    db: sled::Db,
    serial_config: bincode::Config,
}

// TODO: elapsed supports resolution of a nanosecond (but this doesn't mean
// it'll be fully utilized), so collisions between devices that use the same
// UUID should be rare, but are theoretically possible.
// For characteristics like CSC (Cycling Speed and Cadence), it's reasonably
// likely that you have two devices using it--one for speed and the other for
// cadence.
#[derive(Serialize, Deserialize, Debug)]
pub enum Notification {
    Ble((UUID, Vec<u8>)),
    Gps(ParseResult),
}

#[derive(Serialize, Deserialize, Debug)]
enum NotificationType {
    Ble(UUID),
    Gps,
}

pub fn open(path: String) -> sled::Result<TelemetryDb> {
    let db = sled::open(path)?;
    let serial_config = bincode::config().big_endian().clone();
    Ok(TelemetryDb { db, serial_config })
}

pub fn open_default() -> sled::Result<TelemetryDb> {
    open(".rust-cycle.sled".to_string())
}

impl TelemetryDb {
    pub fn insert(
        &self,
        session_key: u64,
        elapsed: Duration,
        notification: Notification,
    ) -> sled::Result<()> {
        let nt = match notification {
            Notification::Gps(_) => NotificationType::Gps,
            Notification::Ble((uuid, _)) => NotificationType::Ble(uuid),
        };
        // I can't imagine why this would fail...
        let key = self
            .serial_config
            .serialize(&(session_key, elapsed, nt))
            .unwrap();
        let value = self.serial_config.serialize(&notification).unwrap();
        self.db.insert(key, value)?;
        Ok(())
    }

    fn decode_key(&self, k: sled::IVec) -> (u64, Duration, NotificationType) {
        // I don't imagine either of these things could fail...
        // Unless there was DB corruption?
        // Maybe good to consider those cases at some point.
        let z: Vec<u8> = (*k).try_into().unwrap();
        let (session_key, d, nt): (u64, Duration, NotificationType) =
            self.serial_config.deserialize(&z).unwrap();
        (session_key, d, nt.into())
    }

    fn decode_value(&self, v: sled::IVec) -> Notification {
        let z: Vec<u8> = (*v).try_into().unwrap();
        self.serial_config.deserialize(&z).unwrap()
    }

    fn decode(
        &self,
        pair: (sled::IVec, sled::IVec),
    ) -> ((u64, Duration, NotificationType), Notification) {
        (self.decode_key(pair.0), self.decode_value(pair.1))
    }

    pub fn get_most_recent_session(&self) -> sled::Result<Option<u64>> {
        self.get_previous_session(u64::max_value())
    }

    pub fn get_previous_session(&self, key: u64) -> sled::Result<Option<u64>> {
        let x = self
            .db
            .get_lt(self.serial_config.serialize(&key).unwrap())?;
        Ok(x.map(|(k, _)| self.decode_key(k).0))
    }

    pub fn get_session_entries(
        &self,
        session_key: u64,
    ) -> impl Iterator<Item = sled::Result<(Duration, Notification)>> + '_ {
        let start = self.serial_config.serialize(&session_key).unwrap();
        let end = self.serial_config.serialize(&(session_key + 1)).unwrap();
        self.db.range(start..end).map(move |x| {
            x.map(|xx| {
                let decoded = self.decode(xx);
                ((decoded.0).1, decoded.1)
            })
        })
    }

    pub fn check_session(&self, key: u64) -> sled::Result<bool> {
        self.db
            .get_gt(self.serial_config.serialize(&key).unwrap())
            .map(|x| x.map_or(false, |(k, _)| self.decode_key(k).0 == key))
    }

    pub fn sessions_between_inclusive(&self, a: u64, b: u64) -> sled::Result<Option<Vec<u64>>> {
        let a_exists = self.check_session(a)?;
        let b_exists = self.check_session(b)?;
        if a > b || !a_exists || !b_exists {
            Ok(None)
        } else {
            if a == b {
                Ok(Some(vec![a]))
            } else {
                let mut v = Vec::new();
                v.push(b);
                let mut last = b;
                loop {
                    // Since we know 'a' exists, it's impossible to _not_ find a previous session
                    last = self.get_previous_session(last)?.unwrap();
                    v.push(last);
                    if last == a {
                        break;
                    }
                }
                v.reverse();
                Ok(Some(v))
            }
        }
    }
}
