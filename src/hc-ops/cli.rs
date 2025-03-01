pub(crate) mod admin;
pub(crate) mod init;
pub(crate) mod tag;

use clap::{Args, Parser, Subcommand};
use std::net::IpAddr;

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Manage tags for Holochain processes
    Tag(TagArgs),

    /// Make an admin call to the conductor
    Admin(AdminArgs),

    /// Check and run app initialisation
    Init(InitArgs),
}

#[derive(Debug, Args)]
pub struct TagArgs {
    #[command(subcommand)]
    pub command: TagCommands,
}

#[derive(Debug, Subcommand)]
pub enum TagCommands {
    /// Tag a Holochain process
    #[command(arg_required_else_help = true)]
    Add {
        /// The address to when connecting to Holochain
        #[arg(long)]
        #[cfg_attr(not(feature = "discover"), arg(required = true))]
        addr: Option<IpAddr>,

        /// The port to use when connecting to Holochain
        #[arg(long)]
        #[cfg_attr(not(feature = "discover"), arg(required = true))]
        port: Option<u16>,

        /// A hint about the process name to search for
        #[cfg(feature = "discover")]
        #[arg(short, long, default_value = "holochain")]
        name: String,

        /// The tag to assign for the selected Holochain admin port
        tag: String,
    },
    /// List all tags
    List,
    Delete {
        /// The tag to delete
        tag: String,
    },
}

#[derive(Debug, Args)]
pub struct AdminArgs {
    /// The tag to use when connecting to Holochain
    #[arg(long, short)]
    pub tag: String,

    #[command(subcommand)]
    pub command: AdminCommands,
}

#[derive(Debug, Subcommand)]
pub enum AdminCommands {
    ListApps,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// The tag to use when connecting to Holochain
    #[arg(long, short)]
    pub tag: String,

    #[command(subcommand)]
    pub command: InitCommands,
}

#[derive(Debug, Subcommand)]
pub enum InitCommands {
    Check,

    #[command(arg_required_else_help = true)]
    Execute {
        /// The app id to initialise cells for
        app_id: String,
    },
}
