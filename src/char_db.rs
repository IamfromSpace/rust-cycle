use btleplug::api::{ValueNotification, UUID};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::time::{Duration, UNIX_EPOCH};

// SUUID is equivalent to a UUID, however it is serializable so we can save its
// value to our sled.
// It's scoped only to this module, so it's mostly hidden.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
enum SUUID {
    B16(u16),
    B128([u8; 16]),
}

impl From<UUID> for SUUID {
    fn from(u: UUID) -> SUUID {
        match u {
            UUID::B16(x) => SUUID::B16(x),
            UUID::B128(x) => SUUID::B128(x),
        }
    }
}

impl From<SUUID> for UUID {
    fn from(u: SUUID) -> UUID {
        match u {
            SUUID::B16(x) => UUID::B16(x),
            SUUID::B128(x) => UUID::B128(x),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CharDb {
    db: sled::Db,
}

pub fn open(path: String) -> sled::Result<CharDb> {
    let db = sled::open(path)?;
    Ok(CharDb { db })
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
        let key = bincode::config()
            .big_endian()
            .serialize(&(session_key, elapsed, SUUID::from(notification.uuid)))
            .unwrap();
        self.db.insert(key, notification.value)?;
        Ok(())
    }

    // Helper function to demonstrate consumption of a DB
    pub fn print_db(&self) -> () {
        for x in self.db.iter() {
            let (k, v) = x.unwrap();
            let z: Vec<u8> = (*k).try_into().unwrap();
            let (session_key, d, suuid): (u64, Duration, SUUID) =
                bincode::config().big_endian().deserialize(&z).unwrap();
            println!(
                "{:?}-{:?}-{:?} = {:?}",
                UNIX_EPOCH
                    .checked_add(Duration::from_secs(session_key))
                    .unwrap(),
                d,
                UUID::from(suuid),
                super::parse_hrm(&(*v).try_into().unwrap())
            );
        }
    }
}
