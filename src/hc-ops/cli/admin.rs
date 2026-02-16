use crate::cli::{AdminArgs, AdminCommands};
use crate::connect_admin_client;
use crate::render::Render;
use diesel::SqliteConnection;
use hc_ops::readable::HumanReadableDisplay;
use holo_hash::DnaHash;
use holochain_client::InstallAppPayload;
use holochain_conductor_api::{AppStatusFilter, CellInfo, StorageBlob, StorageInfo};
use holochain_types::prelude::AppBundleSource;
use kitsune2_api::AgentInfoSigned;
use kitsune2_core::Ed25519Verifier;
use std::collections::HashMap;
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
        AdminCommands::NetworkMetrics { app_id } => {
            let network_metrics = if let Some(app_id) = app_id {
                let dna_hashes = client
                    .list_apps(Some(AppStatusFilter::Enabled))
                    .await?
                    .into_iter()
                    .find(|app| app.installed_app_id == app_id)
                    .map(|app| {
                        app.cell_info
                            .values()
                            .flat_map(|ci| {
                                ci.iter().filter_map(|ci| match ci {
                                    CellInfo::Provisioned(cell) => {
                                        Some(cell.cell_id.dna_hash().clone())
                                    }
                                    _ => None,
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_else(|| Vec::with_capacity(0));

                let mut out = HashMap::with_capacity(dna_hashes.len());
                if dna_hashes.is_empty() {
                    eprintln!("No DNAs found for app: {}", app_id);
                    return Ok(());
                } else {
                    for dna_hash in dna_hashes {
                        let metrics = client.dump_network_metrics(Some(dna_hash), true).await?;
                        out.extend(metrics);
                    }
                }

                out
            } else {
                client.dump_network_metrics(None, true).await?
            };

            std::io::stdout().write_all(network_metrics.as_human_readable()?.as_bytes())?;
        }
        AdminCommands::NetworkStats => {
            let stats = client.dump_network_stats().await?;

            std::io::stdout().write_all(stats.transport_stats.as_human_readable()?.as_bytes())?;
        }
        AdminCommands::ListAgents { app_id } => {
            let agents = if let Some(app_id) = app_id {
                let dna_hashes = client
                    .list_apps(Some(AppStatusFilter::Enabled))
                    .await?
                    .into_iter()
                    .find(|app| app.installed_app_id == app_id)
                    .map(|app| {
                        app.cell_info
                            .values()
                            .flat_map(|ci| {
                                ci.iter().filter_map(|ci| match ci {
                                    CellInfo::Provisioned(cell) => {
                                        Some(cell.cell_id.dna_hash().clone())
                                    }
                                    _ => None,
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_else(|| Vec::with_capacity(0));

                let mut out = HashMap::with_capacity(dna_hashes.len());
                if dna_hashes.is_empty() {
                    eprintln!("No DNA hashes found for app: {}", app_id);
                    return Ok(());
                } else {
                    for dna_hash in dna_hashes {
                        let agents = client.agent_info(Some(vec![dna_hash])).await?;
                        out.extend(
                            agents
                                .into_iter()
                                .filter_map(|s| {
                                    AgentInfoSigned::decode(&Ed25519Verifier, s.as_bytes()).ok()
                                })
                                .map(|a| (DnaHash::from_k2_space(&a.space), a)),
                        );
                    }
                }

                out
            } else {
                client
                    .agent_info(None)
                    .await?
                    .into_iter()
                    .filter_map(|s| AgentInfoSigned::decode(&Ed25519Verifier, s.as_bytes()).ok())
                    .map(|a| (DnaHash::from_k2_space(&a.space), a))
                    .collect()
            };

            std::io::stdout().write_all(agents.as_human_readable()?.as_bytes())?;
        }
    }

    Ok(())
}
