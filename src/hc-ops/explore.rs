use anyhow::{Context, bail};
use diesel::SqliteConnection;
use hc_ops::retrieve::{DbKind, get_some, load_database_key, open_holochain_database};
use holochain_conductor_api::{AppInfo, CellInfo};
use holochain_zome_types::prelude::DnaHash;
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

    let use_db_kind = select_db_kind()?;

    let mut database = open_holochain_database(data_root_path, &use_db_kind, use_dna, key.as_mut())
        .context("Failed to open the selected database")?;

    enum Operation {
        Dump,
        Exit,
    }

    impl Display for Operation {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match self {
                Operation::Dump => write!(f, "Dump"),
                Operation::Exit => write!(f, "Exit"),
            }
        }
    }

    let operations = vec![Operation::Dump, Operation::Exit];
    loop {
        let selected = dialoguer::Select::new()
            .with_prompt("Select an operation")
            .default(0)
            .items(&operations)
            .interact()?;

        match operations[selected] {
            Operation::Dump => {
                let out = get_some(&mut database);
                println!("{:?}", out);
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

pub fn select_db_kind() -> anyhow::Result<DbKind> {
    let selected = dialoguer::Select::new()
        .with_prompt("Select a database kind")
        .default(0)
        .items(&["DHT", "Cache"])
        .interact()?;

    Ok(match selected {
        0 => DbKind::Dht,
        1 => DbKind::Cache,
        _ => unreachable!(),
    })
}
