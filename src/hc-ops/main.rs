use crate::cli::admin::handle_admin_command;
use crate::cli::conductor_tag::handle_conductor_tag_command;
use crate::cli::init::handle_init_command;
use crate::cli::{Cli, Commands};
use crate::data::ConductorTag;
use anyhow::Context;
use clap::Parser;
use diesel::{Connection, SqliteConnection};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness};
// use hc_ops::retrieve::{DbKind, get_some, load_database_key, open_holochain_database};
// use holochain_zome_types::prelude::HoloHashB64;
use crate::cli::agent_tag::handle_agent_tag_command;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::str::FromStr;

const MIGRATIONS: EmbeddedMigrations = diesel_migrations::embed_migrations!();

mod cli;
mod data;
mod interactive;
mod render;
mod schema;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // let pass = rpassword::prompt_password("Enter passphrase: ")?;
    // let pass = sodoken::LockedArray::from(pass.into_bytes());
    // let data_root_path = PathBuf::from("/tmp/nix-shell.jE6AoD/ETq6eeRJCNYoeuQX4G3lM");
    // let mut key = load_database_key(&data_root_path, pass)?;
    //
    // let mut database = open_holochain_database(
    //     &data_root_path,
    //     DbKind::Dht,
    //     HoloHashB64::from_b64_str("uhC0kzneD9a7jv6wLHDK8siExq7J2AKIh2xQWZxFbQmQ74OayV1tE")
    //         .unwrap()
    //         .into(),
    //     key.as_mut(),
    // )?;
    //
    // let out = get_some(&mut database);
    // println!("{:?}", out);

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
    }

    Ok(())
}

async fn connect_admin_client(
    conn: &mut SqliteConnection,
    tag: &str,
) -> anyhow::Result<(holochain_client::AdminWebsocket, ConductorTag)> {
    let tag = data::get_conductor_tag(conn, tag)?
        .ok_or_else(|| anyhow::anyhow!("No such tag: {}", tag))?;

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
