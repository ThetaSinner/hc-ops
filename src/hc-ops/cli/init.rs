use crate::cli::{InitArgs, InitCommands};
use crate::render::Render;
use crate::{connect_admin_client, render};
use diesel::SqliteConnection;
use hc_ops::ops::AdminWebsocketExt;
use holochain_client::ZomeCallTarget;
use holochain_conductor_api::{AppStatusFilter, CellInfo};
use holochain_zome_types::capability::GrantedFunctions;
use holochain_zome_types::init::InitCallbackResult;
use holochain_zome_types::prelude::{ExternIO, FunctionName, ZomeName};
use std::collections::HashSet;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;

pub(crate) async fn handle_init_command(
    conn: &mut SqliteConnection,
    args: InitArgs,
) -> anyhow::Result<()> {
    let (client, tag) = connect_admin_client(conn, &args.tag, &args.origin).await?;

    match args.command {
        InitCommands::Check => {
            let apps = client
                .list_apps(Some(AppStatusFilter::Enabled))
                .await
                .map_err(|e| anyhow::anyhow!("Failed to list apps: {e:?}"))?;

            let mut out = vec![];
            for app in &apps {
                for (role, cells) in &app.cell_info {
                    for cell in cells {
                        match cell {
                            CellInfo::Provisioned(cell) => {
                                let initialised =
                                    client.is_cell_initialized(cell.cell_id.clone()).await?;

                                out.push(render::InitStatus {
                                    app_id: &app.installed_app_id,
                                    role,
                                    dna_hash: cell.cell_id.dna_hash(),
                                    initialised,
                                });
                            }
                            _ => {
                                // Not relevant
                            }
                        }
                    }
                }
            }

            if out.is_empty() {
                eprintln!("No cells to check");
            } else {
                out.render(std::io::stdout())?;
            }
        }
        InitCommands::Execute { origin, app_id } => {
            let signer = Arc::new(holochain_client::ClientAgentSigner::default());
            let app_client = client
                .connect_app_client(
                    IpAddr::from_str(tag.address.as_str())?,
                    app_id.clone(),
                    // TODO Not exposed by the client
                    origin,
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
                            let mut granted = HashSet::<(ZomeName, FunctionName)>::new();
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

    Ok(())
}
