use crate::{HcOpsError, HcOpsResult};
use diesel::{Connection, RunQueryDsl, SqliteConnection, sql_query};
use holo_hash::{ActionHash, EntryHash};
use holochain_types::chain::ChainItem;
use holochain_types::prelude::{DhtOpHash, Entry, SignedActionHashedExt};
use holochain_zome_types::prelude::{AgentPubKey, DnaHash, SignedActionHashed};
use kitsune2_api::{Timestamp, UNIX_TIMESTAMP};
use kitsune2_dht::UNIT_TIME;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

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

pub fn open_conductor_database<P: AsRef<Path>>(
    data_root_path: P,
    key: Option<&mut Key>,
) -> HcOpsResult<SqliteConnection> {
    let path = data_root_path
        .as_ref()
        .join("databases")
        .join("conductor")
        .join("conductor");

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

#[derive(Debug, serde::Deserialize)]
struct ConductorStateView {
    #[serde(default)]
    installed_apps: holochain_types::app::InstalledAppMap,
}

/// Read the list of installed apps directly from the conductor database.
///
/// The conductor persists its state as a single messagepack blob in the
/// `ConductorState` table (id = 1). We decode only the `installed_apps`
/// field, which is enough for the explorer's app/DNA selection.
pub fn get_installed_apps(
    conductor: &mut SqliteConnection,
) -> HcOpsResult<Vec<holochain_types::app::InstalledApp>> {
    #[derive(diesel::QueryableByName)]
    struct Row {
        #[diesel(sql_type = diesel::sql_types::Binary)]
        blob: Vec<u8>,
    }

    let rows: Vec<Row> =
        sql_query("SELECT blob FROM ConductorState WHERE id = 1").load(conductor)?;

    let Some(row) = rows.into_iter().next() else {
        return Ok(Vec::new());
    };

    let view: ConductorStateView = holochain_serialized_bytes::decode(&row.blob)?;
    Ok(view.installed_apps.into_values().collect())
}

pub fn get_blocks(conductor: &mut SqliteConnection) -> HcOpsResult<Vec<BlockRecord>> {
    use diesel::prelude::*;
    use schema::BlockSpan::dsl as block_fields;

    let loaded = schema::BlockSpan::table
        .order_by(block_fields::start_us.asc())
        .select(DbBlockSpan::as_select())
        .load::<DbBlockSpan>(conductor)?;

    loaded.into_iter().map(TryInto::try_into).collect()
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
pub fn get_ops_by_action_hash(
    conn: &mut SqliteConnection,
    action_hash: &ActionHash,
) -> HcOpsResult<Vec<DbDhtOp>> {
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
pub fn get_ops_by_entry_hash(
    conn: &mut SqliteConnection,
    hash: &EntryHash,
) -> HcOpsResult<Vec<DbDhtOp>> {
    use diesel::prelude::*;
    use schema::DhtOp::dsl as dht_op_fields;

    let loaded = schema::DhtOp::table
        .inner_join(
            schema::Action::table.on(dht_op_fields::action_hash
                .assume_not_null()
                .eq(schema::Action::dsl::hash)),
        )
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

/// Get the full chain from the agent using the given authored database connection.
pub fn get_self_agent_chain(authored_conn: &mut SqliteConnection) -> HcOpsResult<Vec<ChainRecord>> {
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
        .order_by(action_fields::seq)
        .load::<(DbAction, Option<Vec<u8>>)>(authored_conn)?;

    let chain: Vec<ChainRecord> = loaded
        .into_iter()
        .map(|l| (l.0, ValidationStatus::Valid, l.1).try_into())
        .collect::<HcOpsResult<Vec<_>>>()?;

    Ok(chain)
}

pub fn get_agent_chain(
    dht_conn: &mut SqliteConnection,
    cache_conn: Option<&mut SqliteConnection>,
    agent_pub_key: &AgentPubKey,
) -> HcOpsResult<Vec<ChainRecord>> {
    let mut chain = get_dht_agent_chain(dht_conn, agent_pub_key)?;

    if let Some(cache_conn) = cache_conn {
        let cache_chain = get_cache_agent_chain(cache_conn, agent_pub_key)?;

        for record in cache_chain {
            merge_into_chain(&mut chain, record);
        }
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

/// Look up the record (op, action, and optional entry) for a given DHT op hash.
///
/// Returns `None` if no op with the given hash exists in the DHT database.
/// Does not filter on integration status — the op is returned whether or not
/// it has been integrated. Chain ops only: warrant ops are not returned.
pub fn get_record_by_op_hash(
    dht: &mut SqliteConnection,
    op_hash: &DhtOpHash,
) -> HcOpsResult<Option<Record>> {
    use diesel::prelude::*;
    use schema::Action::dsl as action_fields;
    use schema::DhtOp::dsl as dht_op_fields;
    use schema::Entry::dsl as entry_fields;

    let op_hash_bytes = op_hash.get_raw_39().to_vec();

    let Some(db_op) = schema::DhtOp::table
        .filter(dht_op_fields::hash.eq(&op_hash_bytes))
        .select(DbDhtOp::as_select())
        .first::<DbDhtOp>(dht)
        .optional()?
    else {
        return Ok(None);
    };

    let Some(action_hash) = db_op.action_hash.clone() else {
        return Ok(None);
    };

    let Some(action_blob) = schema::Action::table
        .filter(action_fields::hash.eq(&action_hash))
        .select(action_fields::blob)
        .first::<Vec<u8>>(dht)
        .optional()?
    else {
        return Ok(None);
    };

    let entry_blob = schema::Action::table
        .inner_join(
            schema::Entry::table.on(action_fields::entry_hash
                .assume_not_null()
                .eq(entry_fields::hash)),
        )
        .filter(action_fields::hash.eq(&action_hash))
        .select(entry_fields::blob)
        .first::<Vec<u8>>(dht)
        .optional()?;

    Ok(Some((db_op, action_blob, entry_blob).try_into()?))
}

/// A warrant op and its associated [`SignedWarrant`] content.
pub struct WarrantRecord {
    pub dht_op: ChainOp<DhtMeta>,
    pub warrant: holochain_zome_types::prelude::SignedWarrant,
}

/// Look up a warrant op and its warrant content by op hash.
///
/// Returns `None` if no op with the given hash exists, if the op is not a warrant op,
/// or if the referenced warrant row is missing.
pub fn get_warrant_by_op_hash(
    dht: &mut SqliteConnection,
    op_hash: &DhtOpHash,
) -> HcOpsResult<Option<WarrantRecord>> {
    use diesel::prelude::*;
    use schema::DhtOp::dsl as dht_op_fields;
    use schema::Warrant::dsl as warrant_fields;

    let op_hash_bytes = op_hash.get_raw_39().to_vec();

    let Some(db_op) = schema::DhtOp::table
        .filter(dht_op_fields::hash.eq(&op_hash_bytes))
        .select(DbDhtOp::as_select())
        .first::<DbDhtOp>(dht)
        .optional()?
    else {
        return Ok(None);
    };

    let Some(warrant_hash) = db_op.action_hash.clone() else {
        return Ok(None);
    };

    let Some(db_warrant) = schema::Warrant::table
        .filter(warrant_fields::hash.eq(&warrant_hash))
        .select(DbWarrant::as_select())
        .first::<DbWarrant>(dht)
        .optional()?
    else {
        return Ok(None);
    };

    let dht_op: ChainOp<DhtMeta> = db_op.try_into()?;
    let warrant: holochain_zome_types::prelude::SignedWarrant = db_warrant.try_into()?;

    Ok(Some(WarrantRecord { dht_op, warrant }))
}

/// List all integrated warrant ops in the DHT database with their warrant content.
///
/// Filters to `DhtOp.typ = "ChainIntegrityWarrant"` with `when_integrated IS NOT NULL`,
/// joined to the `Warrant` row on `DhtOp.action_hash = Warrant.hash`. The validation
/// status (Valid = warrant accepted, Rejected = warrant author was wrong) is carried
/// in `WarrantRecord::dht_op.meta`.
pub fn get_warrants(dht: &mut SqliteConnection) -> HcOpsResult<Vec<WarrantRecord>> {
    use diesel::prelude::*;
    use schema::DhtOp::dsl as dht_op_fields;
    use schema::Warrant::dsl as warrant_fields;

    let loaded = schema::DhtOp::table
        .inner_join(
            schema::Warrant::table.on(dht_op_fields::action_hash
                .assume_not_null()
                .eq(warrant_fields::hash)),
        )
        .filter(dht_op_fields::typ.eq("ChainIntegrityWarrant"))
        .filter(dht_op_fields::when_integrated.is_not_null())
        .order_by(dht_op_fields::authored_timestamp.asc())
        .select((DbDhtOp::as_select(), DbWarrant::as_select()))
        .load::<(DbDhtOp, DbWarrant)>(dht)?;

    loaded
        .into_iter()
        .map(|(db_op, db_warrant)| {
            Ok(WarrantRecord {
                dht_op: db_op.try_into()?,
                warrant: db_warrant.try_into()?,
            })
        })
        .collect()
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

/// Count integrated, valid actions per author in the DHT database.
///
/// Joins `Action` to `DhtOp` and restricts to ops with `when_integrated IS NOT NULL` and
/// `validation_status = Valid`. Each action produces multiple ops, so we count distinct
/// action hashes per author. Results are sorted by count descending, then by agent key.
pub fn count_actions_by_author(dht: &mut SqliteConnection) -> HcOpsResult<Vec<(AgentPubKey, i64)>> {
    use diesel::dsl::count;
    use diesel::prelude::*;
    use schema::Action::dsl as action_fields;
    use schema::DhtOp::dsl as dht_op_fields;

    let loaded = schema::Action::table
        .inner_join(schema::DhtOp::table)
        .filter(dht_op_fields::when_integrated.is_not_null())
        .filter(dht_op_fields::validation_status.eq(ValidationStatus::Valid))
        .group_by(action_fields::author)
        .select((
            action_fields::author,
            count(action_fields::hash).aggregate_distinct(),
        ))
        .load::<(Vec<u8>, i64)>(dht)?;

    let mut out = loaded
        .into_iter()
        .map(|(author, count)| Ok((AgentPubKey::try_from_raw_39(author)?, count)))
        .collect::<HcOpsResult<Vec<_>>>()?;

    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    Ok(out)
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

    #[test]
    fn conductor_state_view_decodes_empty_installed_apps() {
        // A source struct with the same field names as the real
        // holochain::conductor::state::ConductorState. If rmp_serde ever
        // switches from to_vec_named to positional encoding, this test
        // fails before the DB reader hits the same issue.
        #[derive(Debug, serde::Serialize)]
        struct Source {
            tag: String,
            installed_apps: holochain_types::app::InstalledAppMap,
            other_field: u32,
        }

        let src = Source {
            tag: "irrelevant".to_string(),
            installed_apps: Default::default(),
            other_field: 42,
        };

        let bytes = holochain_serialized_bytes::encode(&src).unwrap();
        let view: ConductorStateView = holochain_serialized_bytes::decode(&bytes).unwrap();

        assert!(view.installed_apps.is_empty());
    }

    #[test]
    fn conductor_state_view_round_trips_one_installed_app() {
        use holochain_types::app::{
            AppManifest, AppManifestV0, AppRoleManifest, InstalledApp, InstalledAppCommon,
            InstalledAppMap,
        };
        use holochain_types::prelude::Timestamp;

        let app_id = "my-app".to_string();
        let agent_key = AgentPubKey::from_raw_36(vec![7; 36]);
        let manifest = AppManifest::V0(AppManifestV0 {
            name: "my-app".to_string(),
            description: None,
            roles: vec![AppRoleManifest::sample("role-0".to_string())],
            allow_deferred_memproofs: false,
            bootstrap_url: None,
            signal_url: None,
        });
        let common = InstalledAppCommon::new(
            app_id.clone(),
            agent_key.clone(),
            Vec::<(_, _)>::new(),
            manifest,
            Timestamp::now(),
        )
        .unwrap();
        let installed = InstalledApp::new_fresh(common);

        let mut installed_apps = InstalledAppMap::new();
        installed_apps.insert(app_id.clone(), installed);

        #[derive(Debug, serde::Serialize)]
        struct Source {
            tag: String,
            installed_apps: InstalledAppMap,
        }

        let bytes = holochain_serialized_bytes::encode(&Source {
            tag: "irrelevant".to_string(),
            installed_apps,
        })
        .unwrap();
        let view: ConductorStateView = holochain_serialized_bytes::decode(&bytes).unwrap();

        let apps: Vec<_> = view.installed_apps.into_values().collect();
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].id(), &app_id);
        assert_eq!(apps[0].agent_key, agent_key);
    }
}
