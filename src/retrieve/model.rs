use crate::{HcOpsError, HcOpsResult};
use diesel::backend::Backend;
use diesel::deserialize::FromSql;
use diesel::prelude::*;
use diesel::serialize::{Output, ToSql};
use diesel::sql_types::{SmallInt, Text};
use diesel::{AsExpression, FromSqlRow};
use holochain_zome_types::Entry;
use holochain_zome_types::prelude::{
    ActionHash, AnyLinkableHash, DhtOpHash, SignedAction, Timestamp,
};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = crate::retrieve::schema::DhtOp)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct DbDhtOp {
    pub hash: Vec<u8>,
    pub typ: Option<DhtOpType>,
    pub basis_hash: Option<Vec<u8>>,
    pub action_hash: Option<Vec<u8>>,
    // DHT only
    pub require_receipt: Option<bool>,
    pub storage_center_loc: Option<i32>,
    pub authored_timestamp: Option<i64>,
    pub op_order: String,
    pub validation_status: Option<ValidationStatus>,
    pub when_integrated: Option<i64>,
    // Authored only
    pub withhold_publish: Option<bool>,
    // Authored only
    pub receipts_complete: Option<bool>,
    // Authored only
    pub last_publish_time: Option<i64>,
    // DHT only
    pub validation_stage: Option<ValidationStage>,
    // DHT only
    pub num_validation_attempts: Option<i32>,
    // DHT only
    pub last_validation_attempt: Option<i64>,
    pub dependency: Option<Vec<u8>>,
    // DHT only
    pub when_sys_validated: Option<i32>,
    // DHT only
    pub when_app_validated: Option<i32>,
    // DHT only
    pub when_stored: Option<i32>,
    // DHT only
    pub serialized_size: Option<i32>,
}

#[derive(Debug, Copy, Clone, AsExpression, FromSqlRow, Serialize, Deserialize)]
#[diesel(sql_type = Text)]
pub enum DhtOpType {
    StoreRecord,
    StoreEntry,
    RegisterAgentActivity,
    RegisterUpdatedContent,
    RegisterUpdatedRecord,
    RegisterDeletedBy,
    RegisterDeletedEntryAction,
    RegisterAddLink,
    RegisterRemoveLink,
}

impl<DB: Backend> FromSql<Text, DB> for DhtOpType
where
    String: FromSql<Text, DB>,
{
    fn from_sql(bytes: DB::RawValue<'_>) -> diesel::deserialize::Result<Self> {
        let v = String::from_sql(bytes)?;
        Ok(match v.as_str() {
            "StoreRecord" => DhtOpType::StoreRecord,
            "StoreEntry" => DhtOpType::StoreEntry,
            "RegisterAgentActivity" => DhtOpType::RegisterAgentActivity,
            "RegisterUpdatedContent" => DhtOpType::RegisterUpdatedContent,
            "RegisterUpdatedRecord" => DhtOpType::RegisterUpdatedRecord,
            "RegisterDeletedBy" => DhtOpType::RegisterDeletedBy,
            "RegisterDeletedEntryAction" => DhtOpType::RegisterDeletedEntryAction,
            "RegisterAddLink" => DhtOpType::RegisterAddLink,
            "RegisterRemoveLink" => DhtOpType::RegisterRemoveLink,
            typ => return Err(format!("Unknown DhtOpType: {typ}").into()),
        })
    }
}

#[derive(Debug, Copy, Clone, AsExpression, FromSqlRow, Serialize, Deserialize)]
#[diesel(sql_type = SmallInt)]
pub enum ValidationStage {
    /// Is awaiting to be system validated
    Pending,
    /// Is waiting for dependencies so the op can proceed to system validation
    AwaitingSysDeps,
    /// Is awaiting to be app validated
    SysValidated,
    /// Is waiting for dependencies so the op can proceed to app validation
    AwaitingAppDeps,
    /// Is awaiting to be integrated.
    AwaitingIntegration,
}

impl<DB: Backend> FromSql<SmallInt, DB> for ValidationStage
where
    i16: FromSql<SmallInt, DB>,
{
    fn from_sql(bytes: DB::RawValue<'_>) -> diesel::deserialize::Result<Self> {
        let v = i16::from_sql(bytes)?;
        Ok(match v {
            0 => ValidationStage::Pending,
            1 => ValidationStage::AwaitingSysDeps,
            2 => ValidationStage::SysValidated,
            3 => ValidationStage::AwaitingAppDeps,
            4 => ValidationStage::AwaitingIntegration,
            stage => return Err(format!("Unknown ValidationStage: {stage}").into()),
        })
    }
}

#[derive(Debug, Copy, Clone, AsExpression, FromSqlRow, Serialize, Deserialize)]
#[diesel(sql_type = SmallInt)]
pub enum ValidationStatus {
    /// All dependencies were found and validation passed
    Valid,
    /// Item was rejected by validation
    Rejected,
    /// Holochain has decided to never again attempt validation,
    /// commonly due to missing validation dependencies remaining missing for "too long"
    Abandoned,
}

impl<DB: Backend> FromSql<SmallInt, DB> for ValidationStatus
where
    i16: FromSql<SmallInt, DB>,
{
    fn from_sql(bytes: DB::RawValue<'_>) -> diesel::deserialize::Result<Self> {
        let v = i16::from_sql(bytes)?;
        Ok(match v {
            0 => ValidationStatus::Valid,
            1 => ValidationStatus::Rejected,
            2 => ValidationStatus::Abandoned,
            status => return Err(format!("Unknown ValidationStatus: {status}").into()),
        })
    }
}

impl<DB: Backend> ToSql<SmallInt, DB> for ValidationStatus
where
    i16: ToSql<SmallInt, DB>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, DB>) -> diesel::serialize::Result {
        match self {
            ValidationStatus::Valid => 0i16.to_sql(out),
            ValidationStatus::Rejected => 1i16.to_sql(out),
            ValidationStatus::Abandoned => 2i16.to_sql(out),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DhtMeta {
    pub require_receipt: bool,
    pub validation_stage: Option<ValidationStage>,
    pub num_validation_attempts: Option<u32>,
    pub last_validation_attempt: Option<Timestamp>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheMeta {}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthoredMeta {
    withhold_publish: bool,
    receipts_complete: bool,
    last_publish_time: Option<Timestamp>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DhtOp<Meta = ()>
where
    Meta: Debug,
{
    pub hash: DhtOpHash,
    pub typ: DhtOpType,
    pub basis_hash: AnyLinkableHash,
    pub action_hash: ActionHash,
    pub storage_center_loc: u32,
    pub authored_timestamp: Timestamp,
    pub validation_status: Option<ValidationStatus>,
    pub meta: Meta,
}

impl TryFrom<DbDhtOp> for DhtOp {
    type Error = HcOpsError;

    fn try_from(value: DbDhtOp) -> HcOpsResult<Self> {
        Ok(DhtOp {
            hash: DhtOpHash::try_from_raw_39(value.hash)?,
            typ: value
                .typ
                .ok_or_else(|| HcOpsError::Other("No DhtOpType stored".into()))?,
            basis_hash: AnyLinkableHash::try_from_raw_39(
                value
                    .basis_hash
                    .ok_or_else(|| HcOpsError::Other("No basis hash stored".into()))?,
            )?,
            action_hash: ActionHash::try_from_raw_39(
                value
                    .action_hash
                    .ok_or_else(|| HcOpsError::Other("No action hash stored".into()))?,
            )?,
            storage_center_loc: value
                .storage_center_loc
                .ok_or_else(|| HcOpsError::Other("Missing storage center location".into()))?
                as u32,
            authored_timestamp: value
                .authored_timestamp
                .ok_or_else(|| HcOpsError::Other("Missing authored timestamp".into()))
                .map(Timestamp)?,
            validation_status: value.validation_status,
            meta: (),
        })
    }
}

impl TryFrom<DbDhtOp> for DhtOp<DhtMeta> {
    type Error = HcOpsError;

    fn try_from(value: DbDhtOp) -> HcOpsResult<Self> {
        let dht_meta = DhtMeta {
            // TODO It's a boolean, why is it nullable?
            require_receipt: value.require_receipt.unwrap_or_default(),
            validation_stage: value.validation_stage,
            num_validation_attempts: value.num_validation_attempts.map(|v| v as u32),
            last_validation_attempt: value.last_validation_attempt.map(Timestamp),
        };

        let common: DhtOp = value.try_into()?;

        Ok(DhtOp {
            hash: common.hash,
            typ: common.typ,
            basis_hash: common.basis_hash,
            action_hash: common.action_hash,
            storage_center_loc: common.storage_center_loc,
            authored_timestamp: common.authored_timestamp,
            validation_status: common.validation_status,
            meta: dht_meta,
        })
    }
}

impl TryFrom<DbDhtOp> for DhtOp<CacheMeta> {
    type Error = HcOpsError;

    fn try_from(value: DbDhtOp) -> HcOpsResult<Self> {
        let cache_meta = CacheMeta {};

        let common: DhtOp = value.try_into()?;

        Ok(DhtOp {
            hash: common.hash,
            typ: common.typ,
            basis_hash: common.basis_hash,
            action_hash: common.action_hash,
            storage_center_loc: common.storage_center_loc,
            authored_timestamp: common.authored_timestamp,
            validation_status: common.validation_status,
            meta: cache_meta,
        })
    }
}

impl TryFrom<DbDhtOp> for DhtOp<AuthoredMeta> {
    type Error = HcOpsError;

    fn try_from(value: DbDhtOp) -> HcOpsResult<Self> {
        let authored_meta = AuthoredMeta {
            withhold_publish: value.withhold_publish.unwrap_or_default(),
            receipts_complete: value.receipts_complete.unwrap_or_default(),
            last_publish_time: value.last_publish_time.map(Timestamp),
        };

        let common: DhtOp = value.try_into()?;

        Ok(DhtOp {
            hash: common.hash,
            typ: common.typ,
            basis_hash: common.basis_hash,
            action_hash: common.action_hash,
            storage_center_loc: common.storage_center_loc,
            authored_timestamp: common.authored_timestamp,
            validation_status: common.validation_status,
            meta: authored_meta,
        })
    }
}

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = crate::retrieve::schema::Entry)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
#[allow(dead_code)]
pub struct DbEntry {
    hash: Vec<u8>,
    blob: Vec<u8>,
    tag: Option<String>,
    grantor: Option<Vec<u8>>,
    cap_secret: Option<Vec<u8>>,
    functions: Option<Vec<u8>>,
    access_type: Option<String>,
    access_secret: Option<Vec<u8>>,
    access_assignees: Option<Vec<u8>>,
}

impl TryFrom<DbEntry> for Entry {
    type Error = HcOpsError;

    fn try_from(value: DbEntry) -> HcOpsResult<Self> {
        Ok(holochain_serialized_bytes::decode(value.blob.as_slice())?)
    }
}

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = crate::retrieve::schema::Action)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
#[allow(dead_code)]
pub struct DbAction {
    hash: Vec<u8>,
    typ: String,
    seq: i32,
    author: Vec<u8>,
    blob: Vec<u8>,
    prev_hash: Option<Vec<u8>>,
    entry_hash: Option<Vec<u8>>,
    entry_type: Option<String>,
    private_entry: Option<bool>,
    original_entry_hash: Option<Vec<u8>>,
    original_action_hash: Option<Vec<u8>>,
    deletes_entry_hash: Option<Vec<u8>>,
    deletes_action_hash: Option<Vec<u8>>,
    base_hash: Option<Vec<u8>>,
    zome_index: Option<i32>,
    link_type: Option<i32>,
    tag: Option<Vec<u8>>,
    create_link_hash: Option<Vec<u8>>,
    membrane_proof: Option<Vec<u8>>,
    prev_dna_hash: Option<Vec<u8>>,
}

impl TryFrom<DbAction> for SignedAction {
    type Error = HcOpsError;

    fn try_from(value: DbAction) -> HcOpsResult<Self> {
        Ok(holochain_serialized_bytes::decode(value.blob.as_slice())?)
    }
}

#[derive(Debug, Eq, PartialEq, Queryable, Selectable)]
#[diesel(table_name = crate::retrieve::schema::SliceHash)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct SliceHash {
    pub arc_start: i32,
    pub arc_end: i32,
    pub slice_index: i64,
    pub hash: Vec<u8>,
}

impl PartialOrd for SliceHash {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SliceHash {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.slice_index == other.slice_index {
            return self.arc_start.cmp(&other.arc_start);
        }

        self.slice_index.cmp(&other.slice_index)
    }
}
