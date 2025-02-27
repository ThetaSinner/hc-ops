use crate::cli::{AdminCommands, Cli, Commands, InitCommands, TagCommands};
use crate::data::AddrTag;
use anyhow::Context;
use clap::Parser;
use diesel::{Connection, SqliteConnection};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness};
use hc_ops::ops::AdminWebsocketExt;
use holochain_client::{AppStatusFilter, ZomeCallTarget};
use holochain_conductor_api::CellInfo;
use holochain_zome_types::prelude::{
    ExternIO, FunctionName, GrantedFunctions, InitCallbackResult, ZomeName,
};
use std::collections::BTreeSet;
use std::io::Write;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

const MIGRATIONS: EmbeddedMigrations = diesel_migrations::embed_migrations!();

mod cli;
mod data;
mod interactive;
mod schema;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let db = if let Ok(url) = std::env::var("DATABASE_URL") {
        PathBuf::from(url)
    } else {
        let dir = xdg::BaseDirectories::new()
            .context("Could not discover directory layout")?
            .create_config_directory(Path::new("hc-ops"))
            .context("Failed to create config directory")?;

        std::fs::create_dir_all(&dir).context("Failed to create config directory")?;

        dir.join("state.sqlite3")
    };

    let mut conn =
        SqliteConnection::establish(db.display().to_string().as_str())?;
    conn.run_pending_migrations(MIGRATIONS)
        .map_err(|e| anyhow::anyhow!("Failed to run migrations: {}", e))?;

    match cli.command {
        Commands::Tag(args) => match args.command {
            TagCommands::Add {
                tag,
                addr,
                port,
                #[cfg(feature = "discover")]
                name,
            } => {
                if let (Some(addr), Some(port)) = (addr, port) {
                    data::insert_addr_tag(&mut conn, &tag, SocketAddr::new(addr, port))?;
                } else {
                    #[cfg(feature = "discover")]
                    {
                        let addr = interactive::interactive_discover_holochain_addr(name).await?;
                        data::insert_addr_tag(&mut conn, &tag, addr)?;
                    }
                }
            }
            TagCommands::List => {
                let tags = data::list_addr_tags(&mut conn)?;

                for tag in tags {
                    println!("{: <15}: ws://{}:{}", tag.tag, tag.address, tag.port);
                }
            }
        },
        Commands::Admin(args) => {
            let (client, _) = connect_admin_client(&mut conn, &args.tag).await?;

            match args.command {
                AdminCommands::ListApps => {
                    let apps = client
                        .list_apps(None)
                        .await
                        .map_err(|e| anyhow::anyhow!("Failed to list apps: {e:?}"))?;

                    if apps.is_empty() {
                        eprintln!("No apps installed");
                    } else {
                        let out = serde_json::to_vec(&apps)?;
                        std::io::stdout().write_all(&out)?;
                    }
                }
            }
        }
        Commands::Init(args) => {
            let (client, tag) = connect_admin_client(&mut conn, &args.tag).await?;

            match args.command {
                InitCommands::Check => {
                    let apps = client
                        .list_apps(Some(AppStatusFilter::Running))
                        .await
                        .map_err(|e| anyhow::anyhow!("Failed to list apps: {e:?}"))?;

                    println!(
                        "App Id          Role            DNA hash                                                       Initialised?"
                    );
                    for app in apps {
                        for (role, cells) in app.cell_info {
                            for cell in cells {
                                match cell {
                                    CellInfo::Provisioned(cell) => {
                                        let initialised = client
                                            .is_cell_initialized(cell.cell_id.clone())
                                            .await?;

                                        println!(
                                            "{: <15} {: <15} {:?} {initialised}",
                                            app.installed_app_id,
                                            role,
                                            cell.cell_id.dna_hash()
                                        )
                                    }
                                    _ => {
                                        // Not relevant
                                    }
                                }
                            }
                        }
                    }
                }
                InitCommands::Execute { app_id } => {
                    let signer = Arc::new(holochain_client::ClientAgentSigner::default());
                    let app_client = client
                        .connect_app_client(
                            IpAddr::from_str(tag.address.as_str())?,
                            app_id.clone(),
                            // TODO Not exposed by the client
                            "holochain_websocket", // "hc-ops",
                            signer.clone(),
                        )
                        .await?;

                    let app_infos = client
                        .list_apps(None)
                        .await
                        .map_err(|e| anyhow::anyhow!("Failed to list apps: {e:?}"))?;
                    let app = app_infos
                        .iter()
                        .find(|app| app.installed_app_id == app_id)
                        .ok_or_else(|| anyhow::anyhow!("App not found"))?;

                    println!("Using app info: {:#?}", app);

                    for cells in app.cell_info.values() {
                        for cell in cells {
                            match cell {
                                CellInfo::Provisioned(cell) => {
                                    if client.is_cell_initialized(cell.cell_id.clone()).await? {
                                        println!("Already initialized: {:?}", cell.cell_id);
                                        continue;
                                    }

                                    // TODO No way to retrieve zome names through the AppInfo because of the
                                    //      silly bundle format. You just get a path to a file you don't have...
                                    let zome: String = dialoguer::Input::new()
                                        .with_prompt(format!(
                                            "What zome should be called for: [{:?}]?",
                                            cell.cell_id
                                        ))
                                        .interact_text()?;

                                    // TODO Why does this end up initializing the zomes before we make a call!?
                                    let mut granted = BTreeSet::<(ZomeName, FunctionName)>::new();
                                    granted.insert((zome.clone().into(), "init".into()));
                                    let creds = client
                                        .authorize_signing_credentials(
                                            holochain_client::AuthorizeSigningCredentialsPayload {
                                                cell_id: cell.cell_id.clone(),
                                                functions: Some(GrantedFunctions::Listed(granted)),
                                            },
                                        )
                                        .await?;

                                    signer.add_credentials(cell.cell_id.clone(), creds);

                                    let out = app_client
                                        .call_zome(
                                            ZomeCallTarget::CellId(cell.cell_id.clone()),
                                            zome.into(),
                                            "init".into(),
                                            ExternIO::encode(())?,
                                        )
                                        .await
                                        .map_err(|e| {
                                            anyhow::anyhow!("Failed to call init on zome: {:?}", e)
                                        })?;

                                    println!(
                                        "Init result: {:?}",
                                        ExternIO::decode::<InitCallbackResult>(&out)?
                                    );
                                }
                                _ => {
                                    // Not relevant
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

async fn connect_admin_client(
    conn: &mut SqliteConnection,
    tag: &str,
) -> anyhow::Result<(holochain_client::AdminWebsocket, AddrTag)> {
    let tag = data::get_tag(conn, tag)?.ok_or_else(|| anyhow::anyhow!("No such tag: {}", tag))?;

    let client = holochain_client::AdminWebsocket::connect(SocketAddr::new(
        IpAddr::from_str(&tag.address).context("Invalid IP address stored")?,
        tag.port as u16,
    ))
    .await
    .with_context(|| {
        anyhow::anyhow!("Is Holochain running at: ws://{}:{}", tag.address, tag.port)
    })?;

    Ok((client, tag))
}
