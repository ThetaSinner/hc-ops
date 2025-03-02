mod crypt;
mod model;
mod schema;

use crate::retrieve::model::DhtOp;
use crate::{HcOpsError, HcOpsResult};
use diesel::{Connection, RunQueryDsl, SqliteConnection};
use holochain_zome_types::prelude::DnaHash;
use std::path::Path;

pub enum DbKind {
    Dht,
    Cache,
}

pub fn load_database_key<P: AsRef<Path>>(
    data_root_path: P,
    passphrase: sodoken::LockedArray,
) -> HcOpsResult<Option<crypt::Key>> {
    let db_key = data_root_path.as_ref().join("databases").join("db.key");
    Ok(if db_key.exists() {
        Some(crypt::Key::load(db_key, passphrase)?)
    } else {
        None
    })
}

pub fn open_holochain_database<P: AsRef<Path>>(
    data_root_path: P,
    kind: &DbKind,
    dna_hash: &DnaHash,
    key: Option<&mut crypt::Key>,
) -> HcOpsResult<SqliteConnection> {
    let database_path = data_root_path.as_ref().join("databases");

    let path = match kind {
        DbKind::Dht => database_path.join("dht").join(dna_hash.to_string()),
        DbKind::Cache => database_path.join("cache").join(dna_hash.to_string()),
    };

    let mut conn = SqliteConnection::establish(
        path.to_str()
            .ok_or_else(|| HcOpsError::Other("Invalid database path".into()))?,
    )
    .map_err(HcOpsError::other)?;

    if let Some(key) = key {
        crypt::apply_key(&mut conn, key)?;
    }

    Ok(conn)
}

pub fn get_some(conn: &mut SqliteConnection) -> Vec<DhtOp> {
    schema::DhtOp::table.load(conn).unwrap()
}
