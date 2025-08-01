use crate::cli::admin::handle_admin_command;
use crate::cli::agent_tag::handle_agent_tag_command;
use crate::cli::conductor_tag::handle_conductor_tag_command;
use crate::cli::explore::handle_explore_command;
use crate::cli::init::handle_init_command;
use crate::cli::{Cli, Commands};
use crate::compare::handle_compare_command;
use crate::data::ConductorTag;
use anyhow::Context;
use clap::Parser;
use diesel::{Connection, SqliteConnection};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness};
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::str::FromStr;

const MIGRATIONS: EmbeddedMigrations = diesel_migrations::embed_migrations!();

mod cli;
mod compare;
mod data;
mod explore;
mod interactive;
mod render;
mod schema;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let db = if let Ok(url) = std::env::var("DATABASE_URL") {
        PathBuf::from(url)
    } else {
        let dir = xdg::BaseDirectories::new()
            .create_config_directory(Path::new("hc-ops"))
            .context("Failed to create config directory")?;

        std::fs::create_dir_all(&dir).context("Failed to create config directory")?;

        dir.join("state.sqlite3")
    };

    let mut conn = SqliteConnection::establish(db.display().to_string().as_str())?;
    conn.run_pending_migrations(MIGRATIONS)
        .map_err(|e| anyhow::anyhow!("Failed to run migrations: {}", e))?;

    match cli.command {
        Commands::ConductorTag(args) => {
            handle_conductor_tag_command(&mut conn, args).await?;
        }
        Commands::AgentTag(args) => {
            handle_agent_tag_command(&mut conn, args).await?;
        }
        Commands::Admin(args) => {
            handle_admin_command(&mut conn, args).await?;
        }
        Commands::Init(args) => {
            handle_init_command(&mut conn, args).await?;
        }
        Commands::Explore(args) => {
            handle_explore_command(&mut conn, args).await?;
        }
        Commands::Compare(args) => {
            handle_compare_command(args)?;
        }
    }

    Ok(())
}

async fn connect_admin_client(
    conn: &mut SqliteConnection,
    tag: &str,
    origin: &str,
) -> anyhow::Result<(holochain_client::AdminWebsocket, ConductorTag)> {
    let tag = data::get_conductor_tag(conn, tag)?
        .ok_or_else(|| anyhow::anyhow!("No such tag: {}", tag))?;

    let socket_addr = SocketAddr::new(
        IpAddr::from_str(&tag.address).context("Invalid IP address stored")?,
        tag.port as u16,
    );

    let mut request = holochain_client::ConnectRequest::from(socket_addr);
    request = request
        .try_set_header("Origin", origin)
        .context("Failed to set Origin header")?;

    let client = holochain_client::AdminWebsocket::connect_with_request_and_config(
        request,
        std::sync::Arc::new(holochain_client::WebsocketConfig::CLIENT_DEFAULT),
    )
    .await
    .with_context(|| {
        anyhow::anyhow!("Is Holochain running at: ws://{}:{}", tag.address, tag.port)
    })?;

    Ok((client, tag))
}
