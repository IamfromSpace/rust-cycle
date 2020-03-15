use btleplug::api::{ValueNotification, UUID};
use std::convert::TryInto;
use std::time::Duration;

#[derive(Clone)]
pub struct CharDb {
    db: sled::Db,
    key_coder: bincode::Config,
}

pub fn open(path: String) -> sled::Result<CharDb> {
    let db = sled::open(path)?;
    let key_coder = bincode::config().big_endian().clone();
    Ok(CharDb { db, key_coder })
}

pub fn open_default() -> sled::Result<CharDb> {
    open(".rust-cycle.sled".to_string())
}

impl CharDb {
    pub fn insert(
        &self,
        session_key: u64,
        elapsed: Duration,
        notification: ValueNotification,
    ) -> sled::Result<()> {
        // I can't imagine why this would fail...
        let key = self
            .key_coder
            .serialize(&(session_key, elapsed, notification.uuid))
            .unwrap();
        self.db.insert(key, notification.value)?;
        Ok(())
    }

    fn decode_key(&self, k: sled::IVec) -> (u64, Duration, UUID) {
        // I don't imagine either of these things could fail...
        // Unless there was DB corruption?
        // Maybe good to consider those cases at some point.
        let z: Vec<u8> = (*k).try_into().unwrap();
        let (session_key, d, suuid): (u64, Duration, UUID) =
            self.key_coder.deserialize(&z).unwrap();
        (session_key, d, suuid.into())
    }

    fn decode_value(&self, v: sled::IVec) -> Vec<u8> {
        (*v).try_into().unwrap()
    }

    fn decode(&self, pair: (sled::IVec, sled::IVec)) -> ((u64, Duration, UUID), Vec<u8>) {
        (self.decode_key(pair.0), self.decode_value(pair.1))
    }

    pub fn get_most_recent_session(&self) -> sled::Result<Option<u64>> {
        let x = self
            .db
            .get_lt(self.key_coder.serialize(&u64::max_value()).unwrap())?;
        Ok(x.map(|(k, _)| self.decode_key(k).0))
    }

    pub fn get_session_entries(
        &self,
        session_key: u64,
    ) -> impl Iterator<Item = sled::Result<((u64, Duration, UUID), Vec<u8>)>> + '_ {
        let start = self.key_coder.serialize(&session_key).unwrap();
        let end = self.key_coder.serialize(&(session_key + 1)).unwrap();
        self.db
            .range(start..end)
            .map(move |x| x.map(|xx| self.decode(xx)))
    }
}
