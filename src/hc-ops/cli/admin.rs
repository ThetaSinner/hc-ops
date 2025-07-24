use crate::cli::{AdminArgs, AdminCommands};
use crate::connect_admin_client;
use crate::render::Render;
use diesel::SqliteConnection;
use hc_ops::readable::HumanReadableDisplay;
use holochain_client::InstallAppPayload;
use holochain_conductor_api::{StorageBlob, StorageInfo};
use holochain_types::prelude::AppBundleSource;
use std::io::Write;

pub(crate) async fn handle_admin_command(
    conn: &mut SqliteConnection,
    args: AdminArgs,
) -> anyhow::Result<()> {
    let (client, _) = connect_admin_client(conn, &args.tag, &args.origin).await?;

    match args.command {
        AdminCommands::ListApps { full } => {
            let apps = client
                .list_apps(None)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to list apps: {e:?}"))?;

            if apps.is_empty() {
                eprintln!("No apps installed");
            } else {
                let out = if full {
                    apps.as_human_readable()?
                } else {
                    apps.as_human_readable_summary()?
                };
                std::io::stdout().write_all(out.as_bytes())?;
            }
        }
        AdminCommands::InstallApp {
            path,
            network_seed,
            app_id,
        } => {
            println!("Installing app from path: {:?}", path);

            let installed = client
                .install_app(InstallAppPayload {
                    source: AppBundleSource::Path(path),
                    agent_key: None,
                    installed_app_id: app_id,
                    network_seed,
                    roles_settings: None,
                    ignore_genesis_failure: false,
                    allow_throwaway_random_agent_key: true,
                })
                .await?;

            println!("Installed app under agent: {:?}", installed.agent_pub_key);

            client
                .enable_app(installed.installed_app_id.clone())
                .await?;

            println!("Enabled app: {:?}", installed.installed_app_id);

            println!("Done");
        }
        AdminCommands::UninstallApp { app_id } => {
            println!("Uninstalling app: {:?}", app_id);

            client.uninstall_app(app_id, false).await?;

            println!("Done");
        }
        AdminCommands::StorageInfo { app_id } => {
            println!("Getting storage info");

            let storage_info = client.storage_info().await?;

            let storage_info = match app_id {
                Some(app_id) => {
                    let blobs = storage_info
                        .blobs
                        .into_iter()
                        .filter(|b| match b {
                            StorageBlob::Dna(dna) => dna.used_by.contains(&app_id),
                        })
                        .collect();

                    StorageInfo { blobs }
                }
                None => storage_info,
            };

            if storage_info.blobs.is_empty() {
                eprintln!("No storage info available");
            } else {
                storage_info.render(std::io::stdout())?;
            }
        }
    }

    Ok(())
}
