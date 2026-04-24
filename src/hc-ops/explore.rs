use crate::render::{ActionCountByAuthorTable, Render, SliceHashTable};
use anyhow::Context;
use diesel::SqliteConnection;
use hc_ops::readable::{HumanReadable, HumanReadableDisplay};
use hc_ops::retrieve::{
    AuthoredMeta, CacheMeta, ChainOp, DbKind, DhtMeta, count_actions_by_author, get_agent_chain,
    get_all_actions, get_all_dht_ops, get_all_entries, get_blocks, get_ops_by_action_hash,
    get_ops_by_entry_hash, get_ops_in_slice, get_pending_ops, get_record_by_op_hash,
    get_self_agent_chain, get_slice_hashes, get_warrant_by_op_hash, get_warrants,
    list_discovered_agents, load_database_key, open_conductor_database, open_holochain_database,
};
use hc_ops::{HcOpsError, HcOpsResult};
use holo_hash::{ActionHash, ActionHashB64};
use holochain_conductor_api::{AppInfo, CellInfo};
use holochain_zome_types::prelude::{AgentPubKey, AgentPubKeyB64, DnaHash, Entry, SignedAction};
use std::fmt::{Display, Formatter};
use std::io::Write;
use std::path::{Path, PathBuf};

enum OutputSink {
    Stdout(Box<dyn Write>),
    File {
        path: PathBuf,
        file: Option<std::fs::File>,
    },
}

impl OutputSink {
    fn stdout() -> Self {
        OutputSink::Stdout(Box::new(std::io::stdout()))
    }

    fn is_file(&self) -> bool {
        matches!(self, OutputSink::File { .. })
    }

    fn writer(&mut self) -> anyhow::Result<&mut dyn Write> {
        match self {
            OutputSink::Stdout(w) => Ok(w.as_mut()),
            OutputSink::File { path, file } => {
                if file.is_none() {
                    *file = Some(
                        std::fs::File::create(&*path)
                            .with_context(|| format!("Could not create {}", path.display()))?,
                    );
                }
                Ok(file.as_mut().unwrap())
            }
        }
    }
}

fn prompt_output_sink() -> anyhow::Result<OutputSink> {
    loop {
        let raw: String = dialoguer::Input::new()
            .with_prompt("Output file (empty for stdout)")
            .allow_empty(true)
            .interact_text()?;

        if raw.trim().is_empty() {
            return Ok(OutputSink::stdout());
        }

        let path = PathBuf::from(raw);
        if path.exists() {
            let overwrite = dialoguer::Confirm::new()
                .with_prompt(format!(
                    "{} already exists. Overwrite?",
                    path.display()
                ))
                .default(false)
                .interact()?;
            if !overwrite {
                continue;
            }
        }

        return Ok(OutputSink::File { path, file: None });
    }
}

fn emit_labeled(sink: &mut OutputSink, label: &str, body: &str) -> anyhow::Result<()> {
    let is_file = sink.is_file();
    let w = sink.writer()?;
    if is_file {
        writeln!(w, "{body}")?;
    } else {
        writeln!(w, "{label}: {body}")?;
    }
    Ok(())
}

pub trait AsAnyhowPretty<T> {
    fn into_anyhow(self) -> anyhow::Result<T>;
}

impl<T> AsAnyhowPretty<T> for HcOpsResult<T> {
    fn into_anyhow(self) -> anyhow::Result<T> {
        match self {
            Ok(t) => Ok(t),
            Err(HcOpsError::Context { source, context }) => {
                Err(*source).into_anyhow().context(context)
            }
            Err(e) => Err(anyhow::anyhow!(e.to_string())),
        }
    }
}

pub async fn start_explorer(
    _conn: &mut SqliteConnection,
    client: holochain_client::AdminWebsocket,
    data_root_path: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let data_root_path = data_root_path.as_ref();

    let pass = rpassword::prompt_password("Enter conductor passphrase to unlock databases: ")?;
    let pass = sodoken::LockedArray::from(pass.into_bytes());
    let mut key = load_database_key(data_root_path, pass)?;

    let apps = client.list_apps(None).await?;

    'outer: loop {
        let use_app = select_app(&apps)?;
        if use_app.is_none() {
            break 'outer;
        }
        let use_app = use_app.unwrap();

        loop {
            let use_dna = select_dna(use_app)?;
            if use_dna.is_none() {
                break;
            }
            let use_dna = use_dna.unwrap();

            loop {
                let mut authored = open_holochain_database(
                    data_root_path,
                    &DbKind::Authored(use_app.agent_pub_key.clone()),
                    use_dna,
                    key.as_mut(),
                )
                .context("Failed to open the authored database")?;
                let mut dht =
                    open_holochain_database(data_root_path, &DbKind::Dht, use_dna, key.as_mut())
                        .context("Failed to open the DHT database")?;
                let mut cache =
                    open_holochain_database(data_root_path, &DbKind::Cache, use_dna, key.as_mut())
                        .context("Failed to open the cache database")?;
                let mut conductor = open_conductor_database(data_root_path, key.as_mut())
                    .context("Failed to open the conductor database")?;

                match run_explorer(&mut authored, &mut dht, &mut cache, &mut conductor) {
                    Ok(true) => break 'outer,
                    Ok(false) => {
                        break;
                    }
                    Err(e) => {
                        eprintln!("\nProblem running action: {:?}\n", e);
                    }
                }
            }
        }
    }

    println!("Thank you for exploring!");

    Ok(())
}

fn run_explorer(
    authored: &mut SqliteConnection,
    dht: &mut SqliteConnection,
    cache: &mut SqliteConnection,
    conductor: &mut SqliteConnection,
) -> anyhow::Result<bool> {
    enum Operation {
        WhoIsHere,
        ActionCountByAuthor,
        AgentChain,
        SelfAgentChain,
        Pending,
        FindOpsByActionHash,
        FindOpsByEntryHash,
        FindRecordByOpHash,
        FindWarrantByOpHash,
        ListWarrants,
        ListBlocks,
        SliceHashes,
        OpsInSlice,
        Dump,
        Back,
        Exit,
    }

    impl Display for Operation {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match self {
                Operation::WhoIsHere => write!(f, "Who is here?"),
                Operation::ActionCountByAuthor => write!(f, "Count actions by author"),
                Operation::AgentChain => write!(f, "View an agent chain"),
                Operation::SelfAgentChain => write!(f, "View this agent's chain"),
                Operation::Pending => write!(f, "View ops pending validation or integration"),
                Operation::FindOpsByActionHash => write!(f, "View ops by action hash"),
                Operation::FindOpsByEntryHash => write!(f, "View ops by entry hash"),
                Operation::FindRecordByOpHash => write!(f, "View action and entry by op hash"),
                Operation::FindWarrantByOpHash => write!(f, "View warrant by op hash"),
                Operation::ListWarrants => write!(f, "List warrants in DHT"),
                Operation::ListBlocks => write!(f, "List blocks (conductor)"),
                Operation::SliceHashes => write!(f, "View slice hashes"),
                Operation::OpsInSlice => write!(f, "View ops in a slice"),
                Operation::Dump => write!(f, "Dump"),
                Operation::Back => write!(f, ":back"),
                Operation::Exit => write!(f, ":exit"),
            }
        }
    }

    let operations = vec![
        Operation::WhoIsHere,
        Operation::ActionCountByAuthor,
        Operation::AgentChain,
        Operation::SelfAgentChain,
        Operation::Pending,
        Operation::FindOpsByActionHash,
        Operation::FindOpsByEntryHash,
        Operation::FindRecordByOpHash,
        Operation::FindWarrantByOpHash,
        Operation::ListWarrants,
        Operation::ListBlocks,
        Operation::SliceHashes,
        Operation::OpsInSlice,
        Operation::Dump,
        Operation::Back,
        Operation::Exit,
    ];
    loop {
        let selected = dialoguer::Select::new()
            .with_prompt("Select an operation")
            .default(0)
            .items(&operations)
            .interact()?;

        let op = &operations[selected];
        if matches!(op, Operation::Back) {
            return Ok(false);
        }
        if matches!(op, Operation::Exit) {
            return Ok(true);
        }

        let mut sink = prompt_output_sink()?;

        match op {
            Operation::WhoIsHere => {
                let discovered = list_discovered_agents(dht, cache)?;

                emit_labeled(
                    &mut sink,
                    "Discovered agents",
                    &discovered.as_human_readable_pretty()?,
                )?;
            }
            Operation::ActionCountByAuthor => {
                let counts = count_actions_by_author(dht).into_anyhow()?;

                if counts.is_empty() {
                    println!("No actions found");
                } else {
                    counts
                        .into_iter()
                        .map(Into::into)
                        .collect::<Vec<ActionCountByAuthorTable>>()
                        .render(sink.writer()?)?
                }
            }
            Operation::AgentChain => {
                let key: String = dialoguer::Input::new()
                    .with_prompt("Enter the agent pubkey")
                    .interact()?;

                let key: AgentPubKey = AgentPubKeyB64::from_b64_str(&key)
                    .context("Invalid agent key")?
                    .into();

                // Prompt the user to check whether to include items from the cache.
                let cache = dialoguer::Confirm::new()
                    .with_prompt("Include items from cache?")
                    .interact()?
                    .then_some(&mut *cache);

                let chain = get_agent_chain(dht, cache, &key).into_anyhow()?;

                emit_labeled(
                    &mut sink,
                    "Agent chain",
                    &chain.as_human_readable_pretty().into_anyhow()?,
                )?;
            }
            Operation::SelfAgentChain => {
                let chain = get_self_agent_chain(authored).into_anyhow()?;

                emit_labeled(
                    &mut sink,
                    "This agent's chain",
                    &chain.as_human_readable_pretty().into_anyhow()?,
                )?;
            }
            Operation::Pending => {
                let pending = get_pending_ops(dht)?;

                if pending.is_empty() {
                    println!("No pending ops");
                } else {
                    emit_labeled(
                        &mut sink,
                        "Pending ops",
                        &pending
                            .as_human_readable_pretty()
                            .context("Could not convert pending ops")?,
                    )?;
                }
            }
            Operation::FindOpsByActionHash => {
                let hash: String = dialoguer::Input::new()
                    .with_prompt("Enter the action hash")
                    .interact()?;

                let hash: ActionHash = ActionHashB64::from_b64_str(&hash)
                    .context("Invalid action hash, must be a 39 character base64 string")?
                    .into();

                let ops = get_ops_by_action_hash(dht, &hash)?
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<HcOpsResult<Vec<ChainOp<DhtMeta>>>>()?;

                if ops.is_empty() {
                    println!("No ops found for action hash: {}", hash);
                } else {
                    emit_labeled(
                        &mut sink,
                        &format!("Ops for action hash {hash}"),
                        &ops.as_human_readable_pretty()?,
                    )?;
                }
            }
            Operation::FindOpsByEntryHash => {
                let hash: String = dialoguer::Input::new()
                    .with_prompt("Enter the entry hash")
                    .interact()?;

                let hash: holo_hash::EntryHash = holo_hash::EntryHashB64::from_b64_str(&hash)
                    .context("Invalid entry hash, must be a 39 character base64 string")?
                    .into();

                let ops = get_ops_by_entry_hash(dht, &hash)?
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<HcOpsResult<Vec<ChainOp<DhtMeta>>>>()?;

                if ops.is_empty() {
                    println!("No ops found for entry hash: {}", hash);
                } else {
                    emit_labeled(
                        &mut sink,
                        &format!("Ops for entry hash {hash}"),
                        &ops.as_human_readable_pretty()?,
                    )?;
                }
            }
            Operation::FindRecordByOpHash => {
                let hash: String = dialoguer::Input::new()
                    .with_prompt("Enter the op hash")
                    .interact()?;

                let hash: holo_hash::DhtOpHash = holo_hash::DhtOpHashB64::from_b64_str(&hash)
                    .context("Invalid op hash, must be a 39 character base64 string")?
                    .into();

                match get_record_by_op_hash(dht, &hash).into_anyhow()? {
                    Some(record) => {
                        emit_labeled(
                            &mut sink,
                            &format!("Record for op hash {hash}"),
                            &record.as_human_readable_pretty().into_anyhow()?,
                        )?;
                    }
                    None => {
                        println!("No op found for op hash: {}", hash);
                    }
                }
            }
            Operation::FindWarrantByOpHash => {
                let hash: String = dialoguer::Input::new()
                    .with_prompt("Enter the op hash")
                    .interact()?;

                let hash: holo_hash::DhtOpHash = holo_hash::DhtOpHashB64::from_b64_str(&hash)
                    .context("Invalid op hash, must be a 39 character base64 string")?
                    .into();

                match get_warrant_by_op_hash(dht, &hash).into_anyhow()? {
                    Some(record) => {
                        emit_labeled(
                            &mut sink,
                            &format!("Warrant for op hash {hash}"),
                            &record.as_human_readable_pretty().into_anyhow()?,
                        )?;
                    }
                    None => {
                        println!("No warrant op found for op hash: {}", hash);
                    }
                }
            }
            Operation::ListWarrants => {
                let warrants = get_warrants(dht).into_anyhow()?;

                if warrants.is_empty() {
                    println!("No warrants found");
                } else {
                    emit_labeled(
                        &mut sink,
                        "Warrants",
                        &warrants.as_human_readable_pretty().into_anyhow()?,
                    )?;
                }
            }
            Operation::ListBlocks => {
                let blocks = get_blocks(conductor).into_anyhow()?;

                if blocks.is_empty() {
                    println!("No blocks found");
                } else {
                    emit_labeled(
                        &mut sink,
                        "Blocks",
                        &blocks.as_human_readable_pretty().into_anyhow()?,
                    )?;
                }
            }
            Operation::SliceHashes => {
                let mut slice_hashes = get_slice_hashes(dht)?;

                slice_hashes.sort();

                slice_hashes
                    .into_iter()
                    .map(Into::into)
                    .collect::<Vec<SliceHashTable>>()
                    .render(sink.writer()?)?
            }
            Operation::OpsInSlice => {
                let arc_start: u32 = dialoguer::Input::new()
                    .with_prompt("Enter the arc start")
                    .interact()?;

                let arc_end: u32 = dialoguer::Input::new()
                    .with_prompt("Enter the arc end")
                    .interact()?;

                let slice_index: u64 = dialoguer::Input::new()
                    .with_prompt("Enter the slice index")
                    .interact()?;

                let ops = get_ops_in_slice(dht, arc_start, arc_end, slice_index)?;

                if ops.is_empty() {
                    println!("No ops in slice");
                } else {
                    let w = sink.writer()?;
                    for op in ops {
                        writeln!(w, "{op:?} @ {}", op.get_loc())?;
                    }
                }
            }
            Operation::Dump => {
                dump(&mut sink, authored, dht, cache)?;
            }
            Operation::Back | Operation::Exit => unreachable!(),
        }
    }
}

fn dump(
    sink: &mut OutputSink,
    authored: &mut SqliteConnection,
    dht: &mut SqliteConnection,
    cache: &mut SqliteConnection,
) -> anyhow::Result<()> {
    let authored_ops = get_all_dht_ops(authored)
        .into_iter()
        .map(TryInto::try_into)
        .collect::<HcOpsResult<Vec<ChainOp<AuthoredMeta>>>>()?;
    let authored_actions = get_all_actions(authored)
        .into_iter()
        .map(TryInto::try_into)
        .collect::<HcOpsResult<Vec<SignedAction>>>()?;
    let authored_entries = get_all_entries(authored)
        .into_iter()
        .map(TryInto::try_into)
        .collect::<HcOpsResult<Vec<Entry>>>()?;
    let dht_ops = get_all_dht_ops(dht)
        .into_iter()
        .map(TryInto::try_into)
        .collect::<HcOpsResult<Vec<ChainOp<DhtMeta>>>>()?;
    let dht_actions = get_all_actions(dht)
        .into_iter()
        .map(TryInto::try_into)
        .collect::<HcOpsResult<Vec<SignedAction>>>()?;
    let cache_ops = get_all_dht_ops(cache)
        .into_iter()
        .map(TryInto::try_into)
        .collect::<HcOpsResult<Vec<ChainOp<CacheMeta>>>>()?;
    let cache_actions = get_all_actions(cache)
        .into_iter()
        .map(TryInto::try_into)
        .collect::<HcOpsResult<Vec<SignedAction>>>()?;

    if sink.is_file() {
        let mut envelope = serde_json::Map::new();
        envelope.insert(
            "authored_ops".to_string(),
            authored_ops.as_human_readable_raw()?,
        );
        envelope.insert(
            "authored_actions".to_string(),
            authored_actions.as_human_readable_summary_raw()?,
        );
        envelope.insert(
            "authored_entries".to_string(),
            authored_entries.as_human_readable_summary_raw()?,
        );
        envelope.insert("dht_ops".to_string(), dht_ops.as_human_readable_raw()?);
        envelope.insert(
            "dht_actions".to_string(),
            dht_actions.as_human_readable_summary_raw()?,
        );
        envelope.insert("cache_ops".to_string(), cache_ops.as_human_readable_raw()?);
        envelope.insert(
            "cache_actions".to_string(),
            cache_actions.as_human_readable_summary_raw()?,
        );

        serde_json::to_writer_pretty(sink.writer()?, &serde_json::Value::Object(envelope))?;
        writeln!(sink.writer()?)?;
    } else {
        println!(
            "Authored ops: {}\n\n",
            authored_ops
                .as_human_readable_pretty()
                .context("Could not convert authored ops")?
        );
        println!(
            "Authored actions: {}",
            authored_actions
                .as_human_readable_summary_pretty()
                .context("Could not convert authored actions")?
        );
        println!(
            "Authored entries: {}",
            authored_entries
                .as_human_readable_summary_pretty()
                .context("Could not convert authored entries")?
        );
        println!(
            "DHT ops: {}\n\n",
            serde_json::to_string_pretty(&dht_ops.as_human_readable_raw()?)?
        );
        println!(
            "DHT actions: {}",
            dht_actions.as_human_readable_summary_pretty()?
        );
        println!(
            "Cache ops: {}\n\n",
            cache_ops.as_human_readable_pretty()?
        );
        println!(
            "Cache actions: {}",
            cache_actions.as_human_readable_summary_pretty()?
        );
    }

    Ok(())
}

fn select_app(apps: &[AppInfo]) -> anyhow::Result<Option<&AppInfo>> {
    if apps.is_empty() {
        anyhow::bail!("No apps found");
    }

    let selected = dialoguer::Select::new()
        .with_prompt("Select an app")
        .default(0)
        .items(
            &apps
                .iter()
                .map(|a| a.installed_app_id.clone())
                .collect::<Vec<_>>(),
        )
        .item(":exit")
        .interact()?;

    if selected == apps.len() {
        return Ok(None);
    }

    Ok(Some(&apps[selected]))
}

fn select_dna(app: &AppInfo) -> anyhow::Result<Option<&DnaHash>> {
    let dna_hashes = app
        .cell_info
        .values()
        .flat_map(|cells| {
            cells.iter().filter_map(|c| match c {
                CellInfo::Provisioned(cell) => Some((
                    cell.name.clone(),
                    cell.cell_id.agent_pubkey(),
                    cell.cell_id.dna_hash(),
                )),
                CellInfo::Cloned(cell) => Some((
                    format!("{}/{}", cell.name, cell.clone_id),
                    cell.cell_id.agent_pubkey(),
                    cell.cell_id.dna_hash(),
                )),
                _ => None,
            })
        })
        .collect::<Vec<_>>();

    if dna_hashes.is_empty() {
        eprintln!("No DNAs found");
        return Ok(None);
    }

    let selected = dialoguer::Select::new()
        .with_prompt("Select a DNA")
        .default(0)
        .items(
            &dna_hashes
                .iter()
                .map(|d| format!("{} ({:?}): {:?}", d.0, d.1, d.2))
                .collect::<Vec<_>>(),
        )
        .item(":back")
        .interact()?;

    if selected == dna_hashes.len() {
        return Ok(None);
    }

    Ok(Some(dna_hashes[selected].2))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_path(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "hc-ops-explore-{tag}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn file_sink_does_not_touch_disk_until_first_write() {
        let path = unique_temp_path("lazy-open");
        assert!(!path.exists());

        let sink = OutputSink::File {
            path: path.clone(),
            file: None,
        };
        drop(sink);

        assert!(
            !path.exists(),
            "file should not be created just by constructing the sink"
        );
    }

    #[test]
    fn file_sink_creates_file_on_first_writer_call() {
        let path = unique_temp_path("first-write");
        let _cleanup = RemoveOnDrop(path.clone());

        let mut sink = OutputSink::File {
            path: path.clone(),
            file: None,
        };
        write!(sink.writer().unwrap(), "hello").unwrap();

        assert!(path.exists(), "file should exist after writing");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
    }

    struct RemoveOnDrop(std::path::PathBuf);
    impl Drop for RemoveOnDrop {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}
