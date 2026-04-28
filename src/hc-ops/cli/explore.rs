use crate::cli::ExploreArgs;
use crate::connect_admin_client;
use crate::explore::start_explorer;
use diesel::SqliteConnection;

pub(crate) async fn handle_explore_command(
    conn: &mut SqliteConnection,
    args: ExploreArgs,
) -> anyhow::Result<()> {
    let client = if args.offline {
        None
    } else {
        let tag = args
            .tag
            .as_deref()
            .expect("clap enforces --tag unless --offline");
        let (client, _) = connect_admin_client(conn, tag, &args.origin).await?;
        Some(client)
    };

    start_explorer(conn, client, &args.data_root_path).await?;

    Ok(())
}
