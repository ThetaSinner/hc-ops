use anyhow::{Context, bail};
use diesel::SqliteConnection;
use hc_ops::HcOpsResult;
use hc_ops::readable::HumanReadable;
use hc_ops::retrieve::{
    AuthoredMeta, CacheMeta, DbKind, DhtMeta, DhtOp, get_all_actions, get_all_dht_ops,
    get_all_entries, list_discovered_agents, load_database_key, open_holochain_database,
};
use holochain_conductor_api::{AppInfo, CellInfo};
use holochain_zome_types::prelude::{DnaHash, Entry, SignedAction};
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
    let use_app = select_app(&apps)?;

    let use_dna = select_dna(use_app)?;

    let mut authored = open_holochain_database(
        data_root_path,
        &DbKind::Authored(use_app.agent_pub_key.clone()),
        use_dna,
        key.as_mut(),
    )
    .context("Failed to open the authored database")?;
    let mut dht = open_holochain_database(data_root_path, &DbKind::Dht, use_dna, key.as_mut())
        .context("Failed to open the DHT database")?;
    let mut cache = open_holochain_database(data_root_path, &DbKind::Cache, use_dna, key.as_mut())
        .context("Failed to open the cache database")?;

    enum Operation {
        WhoIsHere,
        Dump,
        Exit,
    }

    impl Display for Operation {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match self {
                Operation::WhoIsHere => write!(f, "Who is here?"),
                Operation::Dump => write!(f, "Dump"),
                Operation::Exit => write!(f, "Exit"),
            }
        }
    }

    let operations = vec![Operation::WhoIsHere, Operation::Dump, Operation::Exit];
    loop {
        let selected = dialoguer::Select::new()
            .with_prompt("Select an operation")
            .default(0)
            .items(&operations)
            .interact()?;

        match operations[selected] {
            Operation::WhoIsHere => {
                let discovered = list_discovered_agents(&mut dht, &mut cache)?;

                println!(
                    "Discovered agents: {}",
                    serde_json::to_string_pretty(&discovered.as_human_readable_raw()?)?
                );
            }
            Operation::Dump => {
                let out = get_all_dht_ops(&mut authored);
                println!(
                    "Authored ops: {}\n\n",
                    serde_json::to_string_pretty(
                        &out.into_iter()
                            .map(TryInto::try_into)
                            .collect::<HcOpsResult<Vec<DhtOp<AuthoredMeta>>>>()?
                            .as_human_readable_raw()?
                    )?
                );

                let out = get_all_actions(&mut authored);
                println!(
                    "Authored actions: {}",
                    serde_json::to_string_pretty(&serde_json::from_str::<serde_json::Value>(
                        &out.into_iter()
                            .map(TryInto::try_into)
                            .collect::<HcOpsResult<Vec<SignedAction>>>()?
                            .as_human_readable_summary()?
                    )?)?
                );

                let out = get_all_entries(&mut authored);
                println!(
                    "Authored entries: {}",
                    serde_json::to_string_pretty(&serde_json::from_str::<serde_json::Value>(
                        &out.into_iter()
                            .map(TryInto::try_into)
                            .collect::<HcOpsResult<Vec<Entry>>>()?
                            .as_human_readable_summary()?
                    )?)?
                );

                let out = get_all_dht_ops(&mut dht);
                println!(
                    "DHT ops: {}\n\n",
                    serde_json::to_string_pretty(
                        &out.into_iter()
                            .map(TryInto::try_into)
                            .collect::<HcOpsResult<Vec<DhtOp<DhtMeta>>>>()?
                            .as_human_readable_raw()?
                    )?
                );

                let out = get_all_actions(&mut dht);
                println!(
                    "DHT actions: {}",
                    serde_json::to_string_pretty(&serde_json::from_str::<serde_json::Value>(
                        &out.into_iter()
                            .map(TryInto::try_into)
                            .collect::<HcOpsResult<Vec<SignedAction>>>()?
                            .as_human_readable_summary()?
                    )?)?
                );

                let out = get_all_dht_ops(&mut cache);
                println!(
                    "Cache ops: {}\n\n",
                    serde_json::to_string_pretty(
                        &out.into_iter()
                            .map(TryInto::try_into)
                            .collect::<HcOpsResult<Vec<DhtOp<CacheMeta>>>>()?
                            .as_human_readable_raw()?
                    )?
                );

                let out = get_all_actions(&mut cache);
                println!(
                    "Cache actions: {}",
                    serde_json::to_string_pretty(&serde_json::from_str::<serde_json::Value>(
                        &out.into_iter()
                            .map(TryInto::try_into)
                            .collect::<HcOpsResult<Vec<SignedAction>>>()?
                            .as_human_readable_summary()?
                    )?)?
                );
            }
            Operation::Exit => {
                println!("Thank you for exploring! Exiting...");
                break;
            }
        }
    }

    Ok(())
}

pub fn select_app(apps: &[AppInfo]) -> anyhow::Result<&AppInfo> {
    if apps.is_empty() {
        anyhow::bail!("No apps found");
    } else if apps.len() == 1 {
        println!(
            "Selecting the only installed app: {}",
            apps[0].installed_app_id
        );
        return Ok(&apps[0]);
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
        .interact()?;

    Ok(&apps[selected])
}

pub fn select_dna(app: &AppInfo) -> anyhow::Result<&DnaHash> {
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
        bail!("No DNAs found");
    } else if dna_hashes.len() == 1 {
        println!("Selecting the only DNA: {:?}", dna_hashes[0].2);
        return Ok(dna_hashes[0].2);
    }

    let selected = dialoguer::Select::new()
        .with_prompt("Select a DNA")
        .default(0)
        .items(
            &dna_hashes
                .iter()
                .map(|d| format!("{} ({:?}) {:?}", d.0, d.1, d.2))
                .collect::<Vec<_>>(),
        )
        .interact()?;

    Ok(dna_hashes[selected].2)
}
