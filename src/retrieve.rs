mod crypt;

pub use crypt::*;
use std::collections::HashSet;

mod model;
pub use model::*;

mod schema;

use crate::{HcOpsError, HcOpsResult};
use diesel::{Connection, RunQueryDsl, SqliteConnection};
use holochain_zome_types::prelude::{AgentPubKey, DnaHash};
use std::path::Path;

pub enum DbKind {
    Authored(AgentPubKey),
    Dht,
    Cache,
}

pub fn load_database_key<P: AsRef<Path>>(
    data_root_path: P,
    passphrase: sodoken::LockedArray,
) -> HcOpsResult<Option<Key>> {
    let db_key = data_root_path.as_ref().join("databases").join("db.key");
    Ok(if db_key.exists() {
        Some(Key::load(db_key, passphrase)?)
    } else {
        None
    })
}

pub fn open_holochain_database<P: AsRef<Path>>(
    data_root_path: P,
    kind: &DbKind,
    dna_hash: &DnaHash,
    key: Option<&mut Key>,
) -> HcOpsResult<SqliteConnection> {
    let database_path = data_root_path.as_ref().join("databases");

    let path = match kind {
        DbKind::Authored(agent_pub_key) => database_path
            .join("authored")
            .join(format!("{}-{}", dna_hash, agent_pub_key)),
        DbKind::Dht => database_path.join("dht").join(dna_hash.to_string()),
        DbKind::Cache => database_path.join("cache").join(dna_hash.to_string()),
    };

    let mut conn = SqliteConnection::establish(
        path.to_str()
            .ok_or_else(|| HcOpsError::Other("Invalid database path".into()))?,
    )
    .map_err(HcOpsError::other)?;

    if let Some(key) = key {
        apply_key(&mut conn, key)?;
    }

    Ok(conn)
}

pub fn get_all_dht_ops(conn: &mut SqliteConnection) -> Vec<DbDhtOp> {
    schema::DhtOp::table.load(conn).unwrap()
}

pub fn get_all_actions(conn: &mut SqliteConnection) -> Vec<DbAction> {
    schema::Action::table.load(conn).unwrap()
}

pub fn get_all_entries(conn: &mut SqliteConnection) -> Vec<DbEntry> {
    schema::Entry::table.load(conn).unwrap()
}

/// Check the DHT and cache databases for `AgentValidationPkg` actions.
pub fn list_discovered_agents(
    dht_conn: &mut SqliteConnection,
    cache_conn: &mut SqliteConnection,
) -> HcOpsResult<Vec<AgentPubKey>> {
    let mut loaded = list_dht_agent_keys(dht_conn)?
        .into_iter()
        .collect::<HashSet<_>>();
    let cache_loaded = list_cache_agent_keys(cache_conn)?
        .into_iter()
        .collect::<HashSet<_>>();

    loaded.extend(cache_loaded);

    let mut out = Vec::with_capacity(loaded.len());
    for v in loaded {
        out.push(AgentPubKey::from_raw_39(v)?);
    }

    Ok(out)
}

fn list_dht_agent_keys(conn: &mut SqliteConnection) -> HcOpsResult<Vec<Vec<u8>>> {
    use diesel::prelude::*;
    use schema::Action::dsl as action_fields;
    use schema::DhtOp::dsl as dht_op_fields;

    let loaded = schema::Action::table
        .select(action_fields::author)
        .distinct()
        .inner_join(schema::DhtOp::table)
        .filter(action_fields::typ.eq("AgentValidationPkg"))
        .filter(dht_op_fields::validation_status.eq(ValidationStatus::Valid))
        .load::<Vec<u8>>(conn)?;

    Ok(loaded)
}

fn list_cache_agent_keys(conn: &mut SqliteConnection) -> HcOpsResult<Vec<Vec<u8>>> {
    use diesel::prelude::*;
    use schema::Action::dsl as action_fields;

    let loaded = schema::Action::table
        .select(action_fields::author)
        .distinct()
        .filter(action_fields::typ.eq("AgentValidationPkg"))
        .load::<Vec<u8>>(conn)?;

    Ok(loaded)
}
