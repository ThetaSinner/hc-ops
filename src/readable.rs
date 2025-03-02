use crate::{HcOpsError, HcOpsResult};
use holochain_conductor_api::AppInfo;
use holochain_zome_types::prelude::{AgentPubKey, DnaHash};

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
