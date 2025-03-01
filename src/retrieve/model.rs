use diesel::backend::Backend;
use diesel::deserialize::FromSql;
use diesel::prelude::*;
use diesel::sql_types::Text;
use diesel::{AsExpression, FromSqlRow};

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = crate::retrieve::schema::DhtOp)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct DhtOp {
    pub hash: Vec<u8>,
    pub typ: Option<DhtOpType>,
    pub basis_hash: Option<Vec<u8>>,
    pub action_hash: Option<Vec<u8>>,
    pub require_receipt: Option<i16>,
    pub storage_center_loc: Option<i32>,
    pub authored_timestamp: Option<i64>,
}

#[derive(Debug, Copy, Clone, AsExpression, FromSqlRow)]
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
