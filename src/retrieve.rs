use crate::{HcOpsError, HcOpsResult};
use diesel::{Connection, RunQueryDsl, SqliteConnection, sql_query};
use holochain_types::chain::ChainItem;
use holochain_types::prelude::{DhtOpHash, Entry, SignedActionHashedExt};
use holochain_zome_types::prelude::{AgentPubKey, DnaHash, SignedActionHashed};
use kitsune2_api::{Timestamp, UNIX_TIMESTAMP};
use kitsune2_dht::UNIT_TIME;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;
use holo_hash::{ActionHash, EntryHash};

mod crypt;
pub use crypt::*;

mod model;
pub use model::*;

mod schema;

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
        println!("Found database key at {}", db_key.display());
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

/// Get all DHT ops for a given action hash.
///
/// This will return all ops for the action, including those that have not yet been integrated into the DHT (i.e. those with `when_integrated` set to null).
pub fn get_ops_by_action_hash(conn: &mut SqliteConnection, action_hash: &ActionHash) -> HcOpsResult<Vec<DbDhtOp>> {
    use diesel::prelude::*;
    use schema::DhtOp::dsl as dht_op_fields;

    let loaded = schema::DhtOp::table
        .filter(dht_op_fields::action_hash.eq(action_hash.get_raw_39()))
        .select(DbDhtOp::as_select())
        .load(conn)?;

    Ok(loaded)
}

pub fn get_all_entries(conn: &mut SqliteConnection) -> Vec<DbEntry> {
    schema::Entry::table.load(conn).unwrap()
}

/// Get all DHT ops for a given entry hash.
///
/// This will return all ops for the entry, including those that have not yet been integrated into the DHT (i.e. those with `when_integrated` set to null).
pub fn get_ops_by_entry_hash(conn: &mut SqliteConnection, hash: &EntryHash) -> HcOpsResult<Vec<DbDhtOp>> {
    use diesel::prelude::*;
    use schema::DhtOp::dsl as dht_op_fields;

    let loaded = schema::DhtOp::table
        .inner_join(schema::Action::table.on(dht_op_fields::action_hash.assume_not_null().eq(schema::Action::dsl::hash)))
        .filter(schema::Action::entry_hash.eq(hash.get_raw_39()))
        .select(DbDhtOp::as_select())
        .load(conn)?;

    Ok(loaded)
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
        out.push(AgentPubKey::try_from_raw_39(v)?);
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainRecord {
    pub action: SignedActionHashed,
    pub validation_status: ValidationStatus,
    pub entry: Option<Entry>,
}

pub fn get_agent_chain(
    dht_conn: &mut SqliteConnection,
    cache_conn: &mut SqliteConnection,
    agent_pub_key: &AgentPubKey,
) -> HcOpsResult<Vec<ChainRecord>> {
    let mut chain = get_dht_agent_chain(dht_conn, agent_pub_key)?;

    let cache_chain = get_cache_agent_chain(cache_conn, agent_pub_key)?;

    for record in cache_chain {
        merge_into_chain(&mut chain, record);
    }

    Ok(chain)
}

fn merge_into_chain(chain: &mut Vec<ChainRecord>, record: ChainRecord) {
    // Skip if already in chain
    if chain
        .iter()
        .any(|c| c.action.as_hash() == record.action.as_hash())
    {
        return;
    }

    let x = chain.iter().enumerate().find_map(|(i, c)| {
        if c.action.seq() > record.action.seq() {
            Some(i)
        } else {
            None
        }
    });

    match x {
        Some(pos) => chain.insert(pos, record),
        None => chain.push(record),
    }
}

fn get_dht_agent_chain(
    conn: &mut SqliteConnection,
    agent_pub_key: &AgentPubKey,
) -> HcOpsResult<Vec<ChainRecord>> {
    use diesel::prelude::*;
    use schema::Action::dsl as action_fields;
    use schema::DhtOp::dsl as dht_op_fields;
    use schema::Entry::dsl as entry_fields;

    let loaded = schema::Action::table
        .inner_join(schema::DhtOp::table)
        .left_join(
            schema::Entry::table.on(action_fields::entry_hash
                .assume_not_null()
                .eq(entry_fields::hash)),
        )
        .select((
            DbAction::as_select(),
            // Isn't null if `when_integrated` is set
            dht_op_fields::validation_status.assume_not_null(),
            entry_fields::blob.nullable(),
        ))
        .distinct()
        .filter(action_fields::author.eq(agent_pub_key.get_raw_39()))
        .filter(dht_op_fields::when_integrated.is_not_null())
        .order_by(action_fields::seq)
        .load::<(DbAction, ValidationStatus, Option<Vec<u8>>)>(conn)?;

    let chain: Vec<ChainRecord> = loaded
        .into_iter()
        .map(|l| l.try_into())
        .collect::<HcOpsResult<Vec<_>>>()?;

    Ok(chain)
}

fn get_cache_agent_chain(
    conn: &mut SqliteConnection,
    agent_pub_key: &AgentPubKey,
) -> HcOpsResult<Vec<ChainRecord>> {
    use diesel::prelude::*;
    use schema::Action::dsl as action_fields;
    use schema::Entry::dsl as entry_fields;

    let loaded = schema::Action::table
        .left_join(
            schema::Entry::table.on(action_fields::entry_hash
                .assume_not_null()
                .eq(entry_fields::hash)),
        )
        .select((DbAction::as_select(), entry_fields::blob.nullable()))
        .distinct()
        .filter(action_fields::author.eq(agent_pub_key.get_raw_39()))
        .order_by(action_fields::seq)
        .load::<(DbAction, Option<Vec<u8>>)>(conn)?;

    let chain: Vec<ChainRecord> = loaded
        .into_iter()
        .map(|l| (l.0, ValidationStatus::Valid, l.1).try_into())
        .collect::<HcOpsResult<Vec<_>>>()?;

    Ok(chain)
}

impl TryFrom<(DbAction, ValidationStatus, Option<Vec<u8>>)> for ChainRecord {
    type Error = HcOpsError;

    fn try_from(
        (db_action, validation_status, maybe_entry): (DbAction, ValidationStatus, Option<Vec<u8>>),
    ) -> Result<Self, Self::Error> {
        Ok(ChainRecord {
            action: SignedActionHashed::from_content_sync(db_action.try_into()?),
            validation_status,
            entry: maybe_entry
                .map(|e| -> HcOpsResult<Entry> { Ok(holochain_serialized_bytes::decode(&e)?) })
                .transpose()?,
        })
    }
}

pub struct Record {
    pub dht_op: ChainOp<DhtMeta>,
    pub action: SignedActionHashed,
    pub entry: Option<Entry>,
}

pub fn get_pending_ops(dht: &mut SqliteConnection) -> HcOpsResult<Vec<Record>> {
    use diesel::prelude::*;
    use schema::Action::dsl as action_fields;
    use schema::DhtOp::dsl as dht_op_fields;
    use schema::Entry::dsl as entry_fields;

    let loaded = schema::DhtOp::table
        .inner_join(schema::Action::table)
        .left_join(
            schema::Entry::table.on(action_fields::entry_hash
                .assume_not_null()
                .eq(entry_fields::hash)),
        )
        .filter(dht_op_fields::when_integrated.is_null())
        .select((
            DbDhtOp::as_select(),
            action_fields::blob,
            entry_fields::blob.nullable(),
        ))
        .load::<(DbDhtOp, Vec<u8>, Option<Vec<u8>>)>(dht)?;

    loaded
        .into_iter()
        .map(TryInto::try_into)
        .collect::<HcOpsResult<Vec<_>>>()
}

impl TryFrom<(DbDhtOp, Vec<u8>, Option<Vec<u8>>)> for Record {
    type Error = HcOpsError;

    fn try_from(
        (dht_op, action_blob, maybe_entry_blob): (DbDhtOp, Vec<u8>, Option<Vec<u8>>),
    ) -> HcOpsResult<Self> {
        Ok(Record {
            dht_op: dht_op.try_into()?,
            action: SignedActionHashed::from_content_sync(holochain_serialized_bytes::decode(
                &action_blob,
            )?),
            entry: maybe_entry_blob
                .map(|e| -> HcOpsResult<Entry> { Ok(holochain_serialized_bytes::decode(&e)?) })
                .transpose()?,
        })
    }
}

pub fn get_slice_hashes(authored: &mut SqliteConnection) -> HcOpsResult<Vec<SliceHash>> {
    use diesel::prelude::*;

    let loaded = schema::SliceHash::table
        .select(SliceHash::as_select())
        .load::<SliceHash>(authored)?;

    Ok(loaded)
}

pub fn get_ops_in_slice(
    dht: &mut SqliteConnection,
    arc_start: u32,
    arc_end: u32,
    slice_index: u64,
) -> HcOpsResult<Vec<DhtOpHash>> {
    use diesel::prelude::*;

    let (time_start, time_end) = time_bounds_for_slice_index(slice_index);

    #[derive(QueryableByName, PartialEq, Debug)]
    #[diesel(table_name = crate::retrieve::schema::DhtOp)]
    #[diesel(check_for_backend(diesel::sqlite::Sqlite))]
    struct SizedOps {
        hash: Vec<u8>,
        storage_center_loc: Option<i32>,
        serialized_size: Option<i32>,
    }

    let mut query = holochain_sqlite::sql::sql_dht::OP_HASHES_IN_TIME_SLICE.to_string();
    query = query.replace("SELECT", "SELECT storage_center_loc,");

    let rows: Vec<SizedOps> = sql_query(query)
        .bind::<diesel::sql_types::Integer, _>(arc_start as i32)
        .bind::<diesel::sql_types::Integer, _>(arc_end as i32)
        .bind::<diesel::sql_types::BigInt, _>(time_start.as_micros())
        .bind::<diesel::sql_types::BigInt, _>(time_end.as_micros())
        .get_results(dht)?;

    println!("Found hashes with locations:");
    for r in &rows {
        println!(
            "{:?} @ {}",
            DhtOpHash::from_raw_39(r.hash.clone()),
            r.storage_center_loc.unwrap() as u32
        );
    }

    println!("\n\n");

    Ok(rows
        .into_iter()
        .map(|o| DhtOpHash::try_from_raw_39(o.hash))
        .collect::<Result<Vec<_>, _>>()?)
}

fn time_bounds_for_slice_index(slice_index: u64) -> (Timestamp, Timestamp) {
    // See [TimePartition::new] in `kitsune2_dht`.
    let full_slice_duration = Duration::from_secs((1u64 << 9) * UNIT_TIME.as_secs());

    // See [TimePartition::time_bounds_for_full_slice_index] in `kitsune2_dht`.
    let start = UNIX_TIMESTAMP + Duration::from_secs(slice_index * full_slice_duration.as_secs());
    let end = start + full_slice_duration;

    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use holochain_zome_types::prelude::*;
    use rand::RngCore;

    fn create_chain_record(i: u8) -> ChainRecord {
        let mut hash = vec![0; 36];
        rand::rng().fill_bytes(&mut hash);

        ChainRecord {
            action: SignedActionHashed::from_content_sync(SignedAction::new(
                Action::Create(Create {
                    author: AgentPubKey::from_raw_36(vec![i; 36]),
                    timestamp: holochain_zome_types::prelude::Timestamp::now(),
                    action_seq: i as u32,
                    prev_action: ActionHash::from_raw_36(vec![i + 1; 36]),
                    entry_type: EntryType::AgentPubKey,
                    entry_hash: EntryHash::from_raw_36(hash),
                    weight: Default::default(),
                }),
                Signature([0; SIGNATURE_BYTES]),
            )),
            validation_status: crate::retrieve::ValidationStatus::Valid,
            entry: None,
        }
    }

    #[test]
    fn merge_missing_record_into_chain() {
        let mut chain = Vec::new();
        for i in 0..5 {
            chain.push(create_chain_record(i));
        }

        let one_record = chain.remove(2);

        merge_into_chain(&mut chain, one_record);

        assert_eq!(chain.len(), 5);

        assert_eq!(
            vec![0, 1, 2, 3, 4],
            chain.iter().map(|c| c.action.seq()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn merge_duplicate_seq_record_into_chain() {
        let mut chain = Vec::new();
        for i in 0..5 {
            chain.push(create_chain_record(i));
        }

        let one_record = create_chain_record(2);
        merge_into_chain(&mut chain, one_record.clone());

        assert_eq!(chain.len(), 6);

        assert_eq!(
            vec![0, 1, 2, 2, 3, 4],
            chain.iter().map(|c| c.action.seq()).collect::<Vec<_>>()
        );
        assert_eq!(
            3,
            chain
                .iter()
                .enumerate()
                .find_map(|(i, c)| {
                    if c.action.as_hash() == one_record.action.as_hash() {
                        Some(i)
                    } else {
                        None
                    }
                })
                .unwrap()
        )
    }

    #[test]
    fn skip_record_already_in_chain() {
        let mut chain = Vec::new();
        for i in 0..5 {
            chain.push(create_chain_record(i));
        }

        let one_record = chain.get(2).cloned().unwrap();

        merge_into_chain(&mut chain, one_record);

        assert_eq!(chain.len(), 5);
    }
}
