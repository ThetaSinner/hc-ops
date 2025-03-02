use crate::cli::{AdminArgs, AdminCommands};
use crate::connect_admin_client;
use diesel::SqliteConnection;
use hc_ops::readable::HumanReadable;
use std::io::Write;

pub(crate) async fn handle_admin_command(
    conn: &mut SqliteConnection,
    args: AdminArgs,
) -> anyhow::Result<()> {
    let (client, _) = connect_admin_client(conn, &args.tag).await?;

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
    }

    Ok(())
}
