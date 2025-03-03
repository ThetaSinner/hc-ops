use crate::retrieve::{ChainRecord, DhtOp, Record};
use crate::{HcOpsError, HcOpsResult, HcOpsResultContextExt};
use holochain_conductor_api::AppInfo;
use holochain_zome_types::prelude::{
    Action, ActionHash, AgentPubKey, AnyDhtHash, DhtOpHash, DnaHash, Entry, EntryHash,
    SignedAction, SignedActionHashed, Timestamp,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fmt::Debug;

pub trait HumanReadable {
    fn as_human_readable_raw(&self) -> HcOpsResult<serde_json::Value>;

    fn as_human_readable_summary_raw(&self) -> HcOpsResult<serde_json::Value>;
}

pub trait HumanReadableDisplay: HumanReadable {
    fn as_human_readable(&self) -> HcOpsResult<String> {
        Ok(serde_json::to_string(&self.as_human_readable_raw()?)?)
    }

    fn as_human_readable_pretty(&self) -> HcOpsResult<String> {
        Ok(serde_json::to_string_pretty(
            &self.as_human_readable_raw()?,
        )?)
    }

    fn as_human_readable_summary(&self) -> HcOpsResult<String> {
        Ok(serde_json::to_string(
            &self.as_human_readable_summary_raw()?,
        )?)
    }

    fn as_human_readable_summary_pretty(&self) -> HcOpsResult<String> {
        Ok(serde_json::to_string_pretty(
            &self.as_human_readable_summary_raw()?,
        )?)
    }
}

impl<T> HumanReadable for Vec<T>
where
    T: HumanReadable,
{
    fn as_human_readable_raw(&self) -> HcOpsResult<serde_json::Value> {
        let out = self
            .iter()
            .map(|item| item.as_human_readable_raw())
            .collect::<HcOpsResult<Vec<_>>>()?;

        Ok(serde_json::Value::Array(out))
    }

    fn as_human_readable_summary_raw(&self) -> HcOpsResult<serde_json::Value> {
        let out = self
            .iter()
            .map(|item| item.as_human_readable_summary_raw())
            .collect::<HcOpsResult<Vec<_>>>()?;

        Ok(serde_json::Value::Array(out))
    }
}

impl<T: HumanReadable> HumanReadableDisplay for Vec<T> {
    fn as_human_readable(&self) -> HcOpsResult<String> {
        let mut out = Vec::with_capacity(self.len());

        for item in self {
            out.push(item.as_human_readable_raw()?);
        }

        Ok(serde_json::to_string(&serde_json::Value::Array(out))?)
    }

    fn as_human_readable_pretty(&self) -> HcOpsResult<String> {
        let mut out = Vec::with_capacity(self.len());

        for item in self {
            out.push(item.as_human_readable_raw()?);
        }

        Ok(serde_json::to_string_pretty(&serde_json::Value::Array(
            out,
        ))?)
    }

    fn as_human_readable_summary(&self) -> HcOpsResult<String> {
        let mut out = Vec::with_capacity(self.len());

        for item in self {
            out.push(item.as_human_readable_summary_raw()?);
        }

        Ok(serde_json::to_string(&serde_json::Value::Array(out))?)
    }

    fn as_human_readable_summary_pretty(&self) -> HcOpsResult<String> {
        let mut out = Vec::with_capacity(self.len());

        for item in self {
            out.push(item.as_human_readable_summary_raw()?);
        }

        Ok(serde_json::to_string_pretty(&serde_json::Value::Array(
            out,
        ))?)
    }
}

impl HumanReadable for AppInfo {
    fn as_human_readable_raw(&self) -> HcOpsResult<serde_json::Value> {
        let mut app_info: serde_json::Value = serde_json::from_str(&serde_json::to_string(&self)?)?;

        replace_field(&mut app_info, "agent_pub_key", transform_agent_pub_key)?;

        for (_, value) in app_info
            .get_mut("cell_info")
            .and_then(|v| v.as_object_mut())
            .ok_or_else(|| HcOpsError::Other("Unexpected cell info format".into()))?
        {
            for cell in value.as_array_mut().unwrap() {
                let cell = cell.as_object_mut().unwrap();

                if let Some(provisioned) = cell.get_mut("provisioned") {
                    replace_field(provisioned, "cell_id", transform_cell_id)?;
                } else if let Some(cloned) = cell.get_mut("cloned") {
                    replace_field(cloned, "cell_id", transform_cell_id)?;
                    replace_field(cloned, "original_dna_hash", transform_dna_hash)?
                }
            }
        }

        Ok(app_info)
    }

    fn as_human_readable_summary_raw(&self) -> HcOpsResult<serde_json::Value> {
        let mut app_info = self.as_human_readable_raw()?;

        app_info.as_object_mut().unwrap().remove("manifest");

        Ok(app_info)
    }
}

impl<S: Debug + Serialize + DeserializeOwned> HumanReadable for DhtOp<S> {
    fn as_human_readable_raw(&self) -> HcOpsResult<serde_json::Value> {
        let mut dht_op: serde_json::Value = serde_json::from_str(&serde_json::to_string(&self)?)?;

        replace_field(&mut dht_op, "hash", transform_dht_op_hash)?;
        replace_field(&mut dht_op, "basis_hash", transform_any_linkable_hash)?;
        replace_field(&mut dht_op, "action_hash", transform_action_hash)?;
        replace_field(&mut dht_op, "authored_timestamp", transform_timestamp)?;

        if let Some(meta) = dht_op.get_mut("meta").and_then(|v| v.as_object_mut()) {
            if let Some(last_validation_attempt) = meta.get("last_validation_attempt") {
                meta["last_validation_attempt"] = transform_timestamp(last_validation_attempt)?;
            }
        }

        Ok(dht_op)
    }

    fn as_human_readable_summary_raw(&self) -> HcOpsResult<serde_json::Value> {
        let mut dht_op = self.as_human_readable_raw()?;

        dht_op.as_object_mut().unwrap().remove("meta");

        Ok(dht_op)
    }
}

impl HumanReadable for Action {
    #[allow(clippy::collapsible_if)]
    fn as_human_readable_raw(&self) -> HcOpsResult<serde_json::Value> {
        let mut action: serde_json::Value = serde_json::from_str(&serde_json::to_string(&self)?)?;

        if let Some(action) = action.as_object_mut() {
            if action.contains_key("author") {
                action["author"] = transform_agent_pub_key(&action["author"])?;
            }

            if action.contains_key("timestamp") {
                action["timestamp"] = transform_timestamp(&action["timestamp"])?;
            }

            if action.contains_key("prev_action") {
                action["prev_action"] = transform_action_hash(&action["prev_action"])?;
            }

            if action.contains_key("entry_hash") {
                action["entry_hash"] = transform_entry_hash(&action["entry_hash"])?;
            }

            if action.contains_key("type") {
                if action["type"] == "Dna" {
                    if action.contains_key("hash") {
                        action["hash"] = transform_dna_hash(&action["hash"])?;
                    }
                }

                if action["type"] == "CreateLink" {
                    if action.contains_key("base_address") {
                        action["base_address"] =
                            transform_any_linkable_hash(&action["base_address"])?;
                    }

                    if action.contains_key("target_address") {
                        action["target_address"] =
                            transform_any_linkable_hash(&action["target_address"])?;
                    }

                    if action.contains_key("tag") {
                        action["tag"] = transform_flatten_byte_array(&action["tag"])?;
                    }
                }

                if action["type"] == "DeleteLink" {
                    if action.contains_key("base_address") {
                        action["base_address"] =
                            transform_any_linkable_hash(&action["base_address"])?;
                    }

                    if action.contains_key("link_add_address") {
                        action["link_add_address"] =
                            transform_action_hash(&action["link_add_address"])?;
                    }
                }

                if action["type"] == "Update" {
                    if action.contains_key("original_action_address") {
                        action["original_action_address"] =
                            transform_action_hash(&action["original_action_address"])?;
                    }

                    if action.contains_key("original_entry_address") {
                        action["original_entry_address"] =
                            transform_entry_hash(&action["original_entry_address"])?;
                    }
                }

                if action["type"] == "Delete" {
                    if action.contains_key("deletes_address") {
                        action["deletes_address"] =
                            transform_action_hash(&action["deletes_address"])?;
                    }

                    if action.contains_key("deletes_entry_address") {
                        action["deletes_entry_address"] =
                            transform_entry_hash(&action["deletes_entry_address"])?;
                    }
                }
            }
        }

        Ok(action)
    }

    fn as_human_readable_summary_raw(&self) -> HcOpsResult<serde_json::Value> {
        self.as_human_readable_raw()
    }
}

impl HumanReadable for SignedAction {
    fn as_human_readable_raw(&self) -> HcOpsResult<serde_json::Value> {
        let mut out = serde_json::Map::new();

        out.insert("data".to_string(), self.action().as_human_readable_raw()?);

        let sig = serde_json::from_str(&serde_json::to_string(&self.signature())?)?;
        out.insert("signature".to_string(), transform_flatten_byte_array(&sig)?);

        Ok(serde_json::Value::Object(out))
    }

    fn as_human_readable_summary_raw(&self) -> HcOpsResult<serde_json::Value> {
        let mut signed_action = self.as_human_readable_raw()?;

        let action = signed_action
            .as_object_mut()
            .and_then(|v| v.get_mut("data"))
            .ok_or_else(|| HcOpsError::Other("Unexpected signed action structure".into()))?;

        if let Some(action) = action.as_object_mut() {
            if action.contains_key("weight") {
                action.remove("weight");
            }
        }

        signed_action
            .as_object_mut()
            .and_then(|v| v.remove("signature"));

        Ok(signed_action)
    }
}

impl HumanReadable for SignedActionHashed {
    fn as_human_readable_raw(&self) -> HcOpsResult<serde_json::Value> {
        let mut out = serde_json::Map::new();

        out.insert(
            "content".to_string(),
            self.hashed.content.as_human_readable_raw()?,
        );
        let hash = serde_json::from_str(&serde_json::to_string(&self.hashed.hash)?)?;
        out.insert("hash".to_string(), transform_action_hash(&hash)?);
        let sig = serde_json::from_str(&serde_json::to_string(&self.signature)?)?;
        out.insert("signature".to_string(), transform_flatten_byte_array(&sig)?);

        Ok(serde_json::Value::Object(out))
    }

    fn as_human_readable_summary_raw(&self) -> HcOpsResult<serde_json::Value> {
        self.as_human_readable_raw()
    }
}

impl HumanReadable for Entry {
    #[allow(clippy::collapsible_if)]
    fn as_human_readable_raw(&self) -> HcOpsResult<serde_json::Value> {
        let mut out: serde_json::Value = serde_json::from_str(&serde_json::to_string(&self)?)?;

        if let Some(out) = out.as_object_mut() {
            if out.contains_key("entry") {
                if out.contains_key("entry_type") {
                    if out["entry_type"] == "Agent" {
                        out["entry"] = transform_agent_pub_key(&out["entry"])?;
                    }
                    if out["entry_type"] == "App" {
                        out["entry"] = transform_msgpack_blob(&out["entry"])
                            .context("Could not convert app entry from msgpack")?;
                    }
                    if out["entry_type"] == "CapClaim" {
                        if let Some(entry) = out["entry"].as_object_mut() {
                            if entry.contains_key("grantor") {
                                entry["grantor"] = transform_agent_pub_key(&entry["grantor"])?;
                            }
                            if entry.contains_key("secret") {
                                entry["secret"] = serde_json::Value::String("...".to_string())
                            }
                        }
                    }
                    if out["entry_type"] == "CapGrant" {
                        if let Some(entry) = out["entry"].as_object_mut() {
                            if let Some(access) =
                                entry.get_mut("access").and_then(|v| v.as_object_mut())
                            {
                                if access.contains_key("Assigned") {
                                    if let Some(assigned) = access["Assigned"].as_object_mut() {
                                        if assigned.contains_key("secret") {
                                            assigned["secret"] =
                                                serde_json::Value::String("...".to_string())
                                        }

                                        if assigned.contains_key("assignees") {
                                            if let Some(assignees) =
                                                assigned["assignees"].as_array_mut()
                                            {
                                                for assignee in assignees {
                                                    *assignee = transform_agent_pub_key(assignee)?;
                                                }
                                            }
                                        }
                                    }
                                } else if access.contains_key("Transferable") {
                                    if let Some(transferable) =
                                        access["Transferable"].as_object_mut()
                                    {
                                        if transferable.contains_key("secret") {
                                            transferable["secret"] =
                                                serde_json::Value::String("...".to_string())
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(out)
    }

    fn as_human_readable_summary_raw(&self) -> HcOpsResult<serde_json::Value> {
        self.as_human_readable_raw()
    }
}

impl HumanReadable for AgentPubKey {
    fn as_human_readable_raw(&self) -> HcOpsResult<serde_json::Value> {
        Ok(serde_json::Value::String(format!("{:?}", self)))
    }

    fn as_human_readable_summary_raw(&self) -> HcOpsResult<serde_json::Value> {
        self.as_human_readable_raw()
    }
}

impl HumanReadable for ChainRecord {
    fn as_human_readable_raw(&self) -> HcOpsResult<serde_json::Value> {
        let mut obj = serde_json::Map::new();
        obj.insert("action".to_string(), self.action.as_human_readable_raw()?);
        obj.insert(
            "validation_status".to_string(),
            serde_json::Value::String(format!("{:?}", self.validation_status)),
        );
        obj.insert(
            "entry".to_string(),
            self.entry
                .as_ref()
                .map(|e: &Entry| -> HcOpsResult<serde_json::Value> { e.as_human_readable_raw() })
                .transpose()?
                .unwrap_or_else(|| serde_json::Value::Null),
        );

        Ok(serde_json::Value::Object(obj))
    }

    fn as_human_readable_summary_raw(&self) -> HcOpsResult<serde_json::Value> {
        self.as_human_readable_raw()
    }
}

impl HumanReadable for Record {
    fn as_human_readable_raw(&self) -> HcOpsResult<serde_json::Value> {
        let mut out = serde_json::Map::new();

        out.insert("dht_op".to_string(), self.dht_op.as_human_readable_raw()?);
        out.insert("action".to_string(), self.action.as_human_readable_raw()?);
        out.insert(
            "entry".to_string(),
            self.entry
                .as_ref()
                .map(|e| e.as_human_readable_raw())
                .transpose()?
                .unwrap_or_else(|| serde_json::Value::Null),
        );

        Ok(serde_json::Value::Object(out))
    }

    fn as_human_readable_summary_raw(&self) -> HcOpsResult<serde_json::Value> {
        let mut out = serde_json::Map::new();

        out.insert(
            "dht_op".to_string(),
            self.dht_op.as_human_readable_summary_raw()?,
        );
        out.insert(
            "action".to_string(),
            self.action.as_human_readable_summary_raw()?,
        );
        out.insert(
            "entry".to_string(),
            self.entry
                .as_ref()
                .map(|e| e.as_human_readable_summary_raw())
                .transpose()?
                .unwrap_or_else(|| serde_json::Value::Null),
        );

        Ok(serde_json::Value::Object(out))
    }
}

fn convert_byte_array(from: &[serde_json::Value]) -> HcOpsResult<Vec<u8>> {
    from.iter()
        .map(|v| {
            v.as_u64()
                .map(|v| v as u8)
                .ok_or_else(|| HcOpsError::Other("Invalid byte array field".into()))
        })
        .collect::<HcOpsResult<Vec<u8>>>()
}

fn replace_field(
    input: &mut serde_json::Value,
    field: &str,
    transform: fn(&serde_json::Value) -> HcOpsResult<serde_json::Value>,
) -> HcOpsResult<()> {
    *input
        .get_mut(field)
        .ok_or_else(|| HcOpsError::Other(format!("Missing field: {field}").into()))? =
        transform(input.get(field).unwrap())?;

    Ok(())
}

fn transform_cell_id(input: &serde_json::Value) -> HcOpsResult<serde_json::Value> {
    let mut out = Vec::with_capacity(2);

    let cell_id = input
        .as_array()
        .ok_or_else(|| HcOpsError::Other("Cannot convert to a cell id, not an array".into()))?;

    if cell_id.len() != 2 {
        return Err(HcOpsError::Other(
            "Invalid cell id, should have two components".into(),
        ));
    }

    out.push(transform_dna_hash(&cell_id[0])?);
    out.push(transform_agent_pub_key(&cell_id[1])?);

    Ok(serde_json::Value::Array(out))
}

fn transform_dna_hash(input: &serde_json::Value) -> HcOpsResult<serde_json::Value> {
    Ok(serde_json::Value::String(format!(
        "{:?}",
        DnaHash::from_raw_39(convert_byte_array(input.as_array().ok_or_else(|| {
            HcOpsError::Other("Cannot convert to a dna hash, not an array".into())
        })?)?)
        .map_err(HcOpsError::other)?
    )))
}

fn transform_agent_pub_key(input: &serde_json::Value) -> HcOpsResult<serde_json::Value> {
    Ok(serde_json::Value::String(format!(
        "{:?}",
        AgentPubKey::from_raw_39(convert_byte_array(input.as_array().ok_or_else(|| {
            HcOpsError::Other("Cannot convert to an agent pub key, not an array".into())
        })?)?)
        .map_err(HcOpsError::other)?
    )))
}

fn transform_dht_op_hash(input: &serde_json::Value) -> HcOpsResult<serde_json::Value> {
    Ok(serde_json::Value::String(format!(
        "{:?}",
        DhtOpHash::from_raw_39(convert_byte_array(input.as_array().ok_or_else(|| {
            HcOpsError::Other("Cannot convert to a dht op hash, not an array".into())
        })?)?)
        .map_err(HcOpsError::other)?
    )))
}

fn transform_any_linkable_hash(input: &serde_json::Value) -> HcOpsResult<serde_json::Value> {
    Ok(serde_json::Value::String(format!(
        "{:?}",
        AnyDhtHash::from_raw_39(convert_byte_array(input.as_array().ok_or_else(|| {
            HcOpsError::Other("Cannot convert to an any dht op hash, not an array".into())
        })?)?)
        .map_err(HcOpsError::other)?
    )))
}

fn transform_action_hash(input: &serde_json::Value) -> HcOpsResult<serde_json::Value> {
    Ok(serde_json::Value::String(format!(
        "{:?}",
        ActionHash::from_raw_39(convert_byte_array(input.as_array().ok_or_else(|| {
            HcOpsError::Other("Cannot convert to an action hash, not an array".into())
        })?)?)
        .map_err(HcOpsError::other)?
    )))
}

fn transform_entry_hash(input: &serde_json::Value) -> HcOpsResult<serde_json::Value> {
    Ok(serde_json::Value::String(format!(
        "{:?}",
        EntryHash::from_raw_39(convert_byte_array(input.as_array().ok_or_else(|| {
            HcOpsError::Other("Cannot convert to an entry hash, not an array".into())
        })?)?)
        .map_err(HcOpsError::other)?
    )))
}

fn transform_timestamp(input: &serde_json::Value) -> HcOpsResult<serde_json::Value> {
    Ok(serde_json::Value::String(
        Timestamp(
            input
                .as_u64()
                .ok_or_else(|| HcOpsError::Other("Invalid timestamp".into()))? as i64,
        )
        .to_string(),
    ))
}

fn transform_msgpack_blob(input: &serde_json::Value) -> HcOpsResult<serde_json::Value> {
    let blob = convert_byte_array(
        input
            .as_array()
            .ok_or_else(|| HcOpsError::Other("Invalid msgpack blob".into()))?,
    )?;

    match holochain_serialized_bytes::decode::<_, serde_json::Value>(&blob) {
        Ok(as_json) => Ok(as_json),
        Err(_) => transform_flatten_byte_array(input),
    }
}

fn transform_flatten_byte_array(input: &serde_json::Value) -> HcOpsResult<serde_json::Value> {
    let arr = input
        .as_array()
        .ok_or_else(|| HcOpsError::Other("Invalid byte array".into()))?;

    Ok(serde_json::Value::String(format!(
        "ByteArray([{}])",
        convert_byte_array(arr)?
            .into_iter()
            .map(|b| b.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    )))
}
