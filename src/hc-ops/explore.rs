use anyhow::Context;
use diesel::SqliteConnection;
use hc_ops::HcOpsResult;
use hc_ops::readable::{HumanReadable, HumanReadableDisplay};
use hc_ops::retrieve::{
    AuthoredMeta, CacheMeta, DbKind, DhtMeta, DhtOp, get_agent_chain, get_all_actions,
    get_all_dht_ops, get_all_entries, list_discovered_agents, load_database_key,
    open_holochain_database,
};
use holochain_conductor_api::{AppInfo, CellInfo};
use holochain_zome_types::prelude::{AgentPubKey, AgentPubKeyB64, DnaHash, Entry, SignedAction};
use std::fmt::{Display, Formatter};
use std::path::Path;

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

                match run_explorer(&mut authored, &mut dht, &mut cache) {
                    Ok(true) => break 'outer,
                    Ok(false) => {
                        break;
                    }
                    Err(e) => {
                        eprintln!("Error: {:#?}", e);
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
) -> anyhow::Result<bool> {
    enum Operation {
        WhoIsHere,
        AgentChain,
        Dump,
        Back,
        Exit,
    }

    impl Display for Operation {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match self {
                Operation::WhoIsHere => write!(f, "Who is here?"),
                Operation::AgentChain => write!(f, "View an agent chain"),
                Operation::Dump => write!(f, "Dump"),
                Operation::Back => write!(f, "Back"),
                Operation::Exit => write!(f, "Exit"),
            }
        }
    }

    let operations = vec![
        Operation::WhoIsHere,
        Operation::AgentChain,
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

        match operations[selected] {
            Operation::WhoIsHere => {
                let discovered = list_discovered_agents(dht, cache)?;

                println!(
                    "Discovered agents: {}",
                    discovered.as_human_readable_pretty()?
                );
            }
            Operation::AgentChain => {
                let key: String = dialoguer::Input::new()
                    .with_prompt("Enter the agent pubkey")
                    .interact()?;

                let key: AgentPubKey = AgentPubKeyB64::from_b64_str(&key)
                    .context("Invalid agent key")?
                    .into();

                let chain = get_agent_chain(dht, cache, &key)?;

                println!("Agent chain: {}", chain.as_human_readable_pretty()?);
            }
            Operation::Dump => {
                let out = get_all_dht_ops(authored);
                println!(
                    "Authored ops: {}\n\n",
                    out.into_iter()
                        .map(TryInto::try_into)
                        .collect::<HcOpsResult<Vec<DhtOp<AuthoredMeta>>>>()?
                        .as_human_readable_pretty()
                        .context("Could not convert authored ops")?
                );

                let out = get_all_actions(authored);
                println!(
                    "Authored actions: {}",
                    out.into_iter()
                        .map(TryInto::try_into)
                        .collect::<HcOpsResult<Vec<SignedAction>>>()?
                        .as_human_readable_summary_pretty()
                        .context("Could not convert authored actions")?
                );

                let out = get_all_entries(authored);
                println!(
                    "Authored entries: {}",
                    out.into_iter()
                        .map(TryInto::try_into)
                        .collect::<HcOpsResult<Vec<Entry>>>()?
                        .as_human_readable_summary_pretty()
                        .context("Could not convert authored entries")?
                );

                let out = get_all_dht_ops(dht);
                println!(
                    "DHT ops: {}\n\n",
                    serde_json::to_string_pretty(
                        &out.into_iter()
                            .map(TryInto::try_into)
                            .collect::<HcOpsResult<Vec<DhtOp<DhtMeta>>>>()?
                            .as_human_readable_raw()?
                    )?
                );

                let out = get_all_actions(dht);
                println!(
                    "DHT actions: {}",
                    out.into_iter()
                        .map(TryInto::try_into)
                        .collect::<HcOpsResult<Vec<SignedAction>>>()?
                        .as_human_readable_summary_pretty()?
                );

                let out = get_all_dht_ops(cache);
                println!(
                    "Cache ops: {}\n\n",
                    out.into_iter()
                        .map(TryInto::try_into)
                        .collect::<HcOpsResult<Vec<DhtOp<CacheMeta>>>>()?
                        .as_human_readable_pretty()?
                );

                let out = get_all_actions(cache);
                println!(
                    "Cache actions: {}",
                    out.into_iter()
                        .map(TryInto::try_into)
                        .collect::<HcOpsResult<Vec<SignedAction>>>()?
                        .as_human_readable_summary_pretty()?
                );
            }
            Operation::Back => {
                return Ok(false);
            }
            Operation::Exit => {
                return Ok(true);
            }
        }
    }
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
