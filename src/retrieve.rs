use crate::{HcOpsError, HcOpsResult};
use diesel::{Connection, RunQueryDsl, SqliteConnection};
use holochain_types::chain::ChainItem;
use holochain_types::prelude::{Entry, SignedActionHashedExt};
use holochain_zome_types::prelude::{AgentPubKey, DnaHash, SignedActionHashed};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

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
                    timestamp: Timestamp::now(),
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
