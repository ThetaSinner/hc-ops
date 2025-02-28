use std::io::Write;
use diesel::SqliteConnection;
use crate::cli::{AdminArgs, AdminCommands};
use crate::connect_admin_client;

pub(crate) async fn handle_admin_command(
    conn: &mut SqliteConnection,
    args: AdminArgs,
) -> anyhow::Result<()> {
    let (client, _) = connect_admin_client(conn, &args.tag).await?;

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

    Ok(())
}