use btleplug::api::UUID;
use nmea0183::ParseResult;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::time::Duration;

#[derive(Clone)]
pub struct CharDb {
    db: sled::Db,
    serial_config: bincode::Config,
}

#[derive(Serialize, Deserialize)]
pub enum Notification {
    Ble((UUID, Vec<u8>)),
    Gps(ParseResult),
}

#[derive(Serialize, Deserialize)]
enum NotificationType {
    Ble(UUID),
    Gps,
}

pub fn open(path: String) -> sled::Result<CharDb> {
    let db = sled::open(path)?;
    let serial_config = bincode::config().big_endian().clone();
    Ok(CharDb { db, serial_config })
}

pub fn open_default() -> sled::Result<CharDb> {
    open(".rust-cycle.sled".to_string())
}

impl CharDb {
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
        let value = self
            .serial_config
            .serialize(&(session_key, elapsed, notification))
            .unwrap();
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
        let x = self
            .db
            .get_lt(self.serial_config.serialize(&u64::max_value()).unwrap())?;
        Ok(x.map(|(k, _)| self.decode_key(k).0))
    }

    pub fn get_session_entries(
        &self,
        session_key: u64,
    ) -> impl Iterator<Item = sled::Result<((Duration, Notification))>> + '_ {
        let start = self.serial_config.serialize(&session_key).unwrap();
        let end = self.serial_config.serialize(&(session_key + 1)).unwrap();
        self.db.range(start..end).map(move |x| {
            x.map(|xx| {
                let decoded = self.decode(xx);
                ((decoded.0).1, decoded.1)
            })
        })
    }
}
