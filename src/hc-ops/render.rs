use crate::data::{AgentTag, ConductorTag};
use holochain_conductor_api::{StorageBlob, StorageInfo};
use holochain_zome_types::prelude::{AgentPubKey, DnaHash};
use std::io;
use std::io::Write;
use tabled::settings::Style;
use tabled::{Table, Tabled};

fn flush(mut write: impl Write) -> io::Result<()> {
    let _ = write.write(b"\n")?;
    write.flush()?;
    Ok(())
}

#[derive(Tabled)]
pub struct InitStatus<'a> {
    pub app_id: &'a str,
    pub role: &'a str,
    pub dna_hash: &'a DnaHash,
    pub initialised: bool,
}

pub trait Render {
    fn render(&self, write: impl Write) -> io::Result<()>;
}

impl<Item> Render for Vec<Item>
where
    Item: Tabled,
{
    fn render(&self, mut write: impl Write) -> io::Result<()> {
        let _ = write.write(
            Table::new(self)
                .with(Style::modern_rounded())
                .to_string()
                .as_bytes(),
        )?;
        flush(write)
    }
}

#[derive(Tabled)]
pub struct StorageInfoBlob {
    pub referenced_by_apps: String,
    // TODO Holochain API does not return which DNA is which!
    pub dna: String,
    pub authored: String,
    pub authored_on_disk: String,
    pub dht: String,
    pub dht_on_disk: String,
    pub cache: String,
    pub cache_on_disk: String,
}

impl Render for StorageInfo {
    fn render(&self, write: impl Write) -> io::Result<()> {
        let t = self
            .blobs
            .iter()
            .map(|b| match b {
                StorageBlob::Dna(dna) => StorageInfoBlob {
                    referenced_by_apps: dna.used_by.join(", "),
                    dna: "unknown".to_string(),
                    authored: human_bytes::human_bytes(dna.authored_data_size as f64),
                    authored_on_disk: human_bytes::human_bytes(
                        dna.authored_data_size_on_disk as f64,
                    ),
                    dht: human_bytes::human_bytes(dna.dht_data_size as f64),
                    dht_on_disk: human_bytes::human_bytes(dna.dht_data_size_on_disk as f64),
                    cache: human_bytes::human_bytes(dna.cache_data_size as f64),
                    cache_on_disk: human_bytes::human_bytes(dna.cache_data_size_on_disk as f64),
                },
            })
            .collect::<Vec<_>>();

        t.render(write)
    }
}

#[derive(Tabled)]
pub struct AgentTagTable {
    pub agent: String,
    pub tag: String,
}

impl From<AgentTag> for AgentTagTable {
    fn from(tag: AgentTag) -> Self {
        Self {
            agent: format!(
                "{:?}",
                AgentPubKey::from_raw_39(tag.agent).expect("Invalid agent key stored")
            ),
            tag: tag.tag,
        }
    }
}

#[derive(Tabled)]
pub struct ConductorTagTable {
    pub tag: String,
    pub address: String,
    pub port: i32,
}

impl From<ConductorTag> for ConductorTagTable {
    fn from(tag: ConductorTag) -> Self {
        Self {
            tag: tag.tag,
            address: tag.address,
            port: tag.port,
        }
    }
}
