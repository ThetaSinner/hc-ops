use crate::retrieve::DhtOp;
use crate::{HcOpsError, HcOpsResult};
use holochain_conductor_api::AppInfo;
use holochain_zome_types::prelude::{
    ActionHash, AgentPubKey, AnyDhtHash, DhtOpHash, DnaHash, Entry, EntryHash, SignedAction,
    Timestamp,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fmt::Debug;

pub trait HumanReadable {
    fn as_human_readable_raw(&self) -> HcOpsResult<serde_json::Value>;

    fn as_human_readable(&self) -> HcOpsResult<String>;

    fn as_human_readable_summary(&self) -> HcOpsResult<String>;
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

    fn as_human_readable(&self) -> HcOpsResult<String> {
        Ok(serde_json::to_string(&self.as_human_readable_raw()?)?)
    }

    fn as_human_readable_summary(&self) -> HcOpsResult<String> {
        let mut vec = Vec::<serde_json::Value>::with_capacity(self.len());
        for item in self {
            vec.push(serde_json::from_str(&item.as_human_readable_summary()?)?);
        }

        Ok(serde_json::to_string(&vec)?)
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

    fn as_human_readable(&self) -> HcOpsResult<String> {
        let app_info = self.as_human_readable_raw()?;
        Ok(serde_json::to_string(&app_info)?)
    }

    fn as_human_readable_summary(&self) -> HcOpsResult<String> {
        let mut app_info = self.as_human_readable_raw()?;

        app_info.as_object_mut().unwrap().remove("manifest");

        Ok(serde_json::to_string(&app_info)?)
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

    fn as_human_readable(&self) -> HcOpsResult<String> {
        let app_info = self.as_human_readable_raw()?;
        Ok(serde_json::to_string(&app_info)?)
    }

    fn as_human_readable_summary(&self) -> HcOpsResult<String> {
        let mut dht_op = self.as_human_readable_raw()?;

        dht_op.as_object_mut().unwrap().remove("meta");

        Ok(serde_json::to_string(&dht_op)?)
    }
}

impl HumanReadable for SignedAction {
    #[allow(clippy::collapsible_if)]
    fn as_human_readable_raw(&self) -> HcOpsResult<serde_json::Value> {
        let mut out: serde_json::Value = serde_json::from_str(&serde_json::to_string(&self)?)?;

        if let Some(data) = out.get_mut("data").and_then(|v| v.as_object_mut()) {
            if data.contains_key("author") {
                data["author"] = transform_agent_pub_key(&data["author"])?;
            }

            if data.contains_key("timestamp") {
                data["timestamp"] = transform_timestamp(&data["timestamp"])?;
            }

            if data.contains_key("prev_action") {
                data["prev_action"] = transform_action_hash(&data["prev_action"])?;
            }

            if data.contains_key("entry_hash") {
                data["entry_hash"] = transform_entry_hash(&data["entry_hash"])?;
            }

            if data.contains_key("type") {
                if data["type"] == "Dna" {
                    if data.contains_key("hash") {
                        data["hash"] = transform_dna_hash(&data["hash"])?;
                    }
                }

                if data["type"] == "CreateLink" {
                    if data.contains_key("base_address") {
                        data["base_address"] = transform_any_linkable_hash(&data["base_address"])?;
                    }

                    if data.contains_key("target_address") {
                        data["target_address"] =
                            transform_any_linkable_hash(&data["target_address"])?;
                    }

                    if data.contains_key("tag") {
                        data["tag"] = transform_flatten_byte_array(&data["tag"])?;
                    }
                }

                if data["type"] == "DeleteLink" {
                    if data.contains_key("base_address") {
                        data["base_address"] = transform_any_linkable_hash(&data["base_address"])?;
                    }

                    if data.contains_key("link_add_address") {
                        data["link_add_address"] =
                            transform_action_hash(&data["link_add_address"])?;
                    }
                }

                if data["type"] == "Update" {
                    if data.contains_key("original_action_address") {
                        data["original_action_address"] =
                            transform_action_hash(&data["original_action_address"])?;
                    }

                    if data.contains_key("original_entry_address") {
                        data["original_entry_address"] =
                            transform_entry_hash(&data["original_entry_address"])?;
                    }
                }

                if data["type"] == "Delete" {
                    if data.contains_key("deletes_address") {
                        data["deletes_address"] = transform_action_hash(&data["deletes_address"])?;
                    }

                    if data.contains_key("deletes_entry_address") {
                        data["deletes_entry_address"] =
                            transform_entry_hash(&data["deletes_entry_address"])?;
                    }
                }
            }
        }

        Ok(out)
    }

    fn as_human_readable(&self) -> HcOpsResult<String> {
        let signed_action = self.as_human_readable_raw()?;
        Ok(serde_json::to_string(&signed_action)?)
    }

    fn as_human_readable_summary(&self) -> HcOpsResult<String> {
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

        Ok(serde_json::to_string(action)?)
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
                        out["entry"] = transform_msgpack_blob(&out["entry"])?;
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

    fn as_human_readable(&self) -> HcOpsResult<String> {
        let entry = self.as_human_readable_raw()?;
        Ok(serde_json::to_string(&entry)?)
    }

    fn as_human_readable_summary(&self) -> HcOpsResult<String> {
        let entry = self.as_human_readable_raw()?;
        Ok(serde_json::to_string(&entry)?)
    }
}

impl HumanReadable for AgentPubKey {
    fn as_human_readable_raw(&self) -> HcOpsResult<serde_json::Value> {
        Ok(serde_json::Value::String(format!("{:?}", self)))
    }

    fn as_human_readable(&self) -> HcOpsResult<String> {
        Ok(serde_json::to_string(&self.as_human_readable_raw()?)?)
    }

    fn as_human_readable_summary(&self) -> HcOpsResult<String> {
        self.as_human_readable()
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

    let as_json = holochain_serialized_bytes::decode::<_, serde_json::Value>(&blob)?;

    Ok(as_json)
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
