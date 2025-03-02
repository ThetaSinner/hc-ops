pub(crate) mod admin;
pub(crate) mod agent_tag;
pub(crate) mod conductor_tag;
pub(crate) mod init;

use clap::{Args, Parser, Subcommand};
use holochain_zome_types::prelude::AgentPubKeyB64;
use std::net::IpAddr;
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Tag a Holochain conductor
    ConductorTag(ConductorTagArgs),

    /// Tag an agent to make it easier to identify them in output
    AgentTag(AgentTagArgs),

    /// Make an admin call to the conductor
    Admin(AdminArgs),

    /// Check and run app initialisation
    Init(InitArgs),
}

#[derive(Debug, Args)]
pub struct ConductorTagArgs {
    #[command(subcommand)]
    pub command: ConductorTagCommands,
}

#[derive(Debug, Subcommand)]
pub enum ConductorTagCommands {
    /// Tag a Holochain address
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
pub struct AgentTagArgs {
    #[command(subcommand)]
    pub command: AgentTagCommands,
}

#[derive(Debug, Subcommand)]
pub enum AgentTagCommands {
    /// Tag an agent
    #[command(arg_required_else_help = true)]
    Add {
        /// The agent to tag
        agent: AgentPubKeyB64,

        /// The tag to assign
        tag: String,
    },
    /// List all tags
    List,
    /// Delete a tag
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
    /// List installed apps
    ListApps {
        /// Get full output, rather than the default summary
        #[arg(long)]
        full: bool,
    },
    /// Install and enable an app
    #[command(arg_required_else_help = true)]
    InstallApp {
        /// The path to a .happ file to install
        path: PathBuf,

        /// Set a network seed for the app
        network_seed: Option<String>,

        /// Override the app id that the app will be installed under
        app_id: Option<String>,
    },
    /// Uninstall an app
    #[command(arg_required_else_help = true)]
    UninstallApp {
        /// The app id to uninstall
        app_id: String,
    },
    /// Get storage info for apps
    StorageInfo {
        /// Get storage info for a single app
        app_id: Option<String>,
    },
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
