use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(version, about)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Args)]
pub struct GlobalArgs {
    #[arg(long, global = true, env = "FERR0_URL", help = "Mem0 server URL")]
    pub url: Option<String>,
    #[arg(
        long,
        global = true,
        env = "FERR0_API_KEY",
        hide_env_values = true,
        help = "Mem0 API key"
    )]
    pub api_key: Option<String>,
    #[arg(long, global = true, env = "FERR0_USER_ID", help = "Scope to a user")]
    pub user_id: Option<String>,
    #[arg(
        long,
        global = true,
        env = "FERR0_AGENT_ID",
        help = "Scope to an agent"
    )]
    pub agent_id: Option<String>,
    #[arg(long, global = true, env = "FERR0_RUN_ID", help = "Scope to a run")]
    pub run_id: Option<String>,
    #[arg(long, global = true, help = "Print the raw server response as JSON")]
    pub json: bool,
}

#[derive(Subcommand)]
pub enum Command {
    #[command(flatten)]
    Memory(MemoryCommand),
    #[command(subcommand, about = "Manage the config file")]
    Config(ConfigCommand),
    #[command(about = "Interactively configure ferr0 and install the agent skill")]
    Setup,
}

#[derive(Subcommand)]
pub enum MemoryCommand {
    #[command(about = "Add a memory")]
    Add {
        #[arg(help = "Memory text, sent as a user message; read from stdin when omitted")]
        text: Option<String>,
        #[arg(
            long,
            conflicts_with_all = ["text", "file"],
            help = "Messages as a JSON array of {\"role\", \"content\"} objects"
        )]
        messages: Option<String>,
        #[arg(
            long,
            conflicts_with = "text",
            help = "Read the messages JSON array from a file"
        )]
        file: Option<PathBuf>,
    },
    #[command(about = "Search memories")]
    Search {
        #[arg(help = "Search query; read from stdin when omitted")]
        query: Option<String>,
        #[arg(long, help = "Maximum number of results")]
        limit: Option<u32>,
    },
    #[command(about = "List memories")]
    List {
        #[arg(long, help = "Maximum number of results")]
        limit: Option<u32>,
    },
    #[command(about = "Show a memory")]
    Get { id: String },
    #[command(about = "Replace the text of a memory")]
    Update { id: String, text: String },
    #[command(about = "Delete a memory or all memories matching scope filters")]
    Delete(DeleteArgs),
}

#[derive(Args)]
pub struct DeleteArgs {
    #[arg(
        required_unless_present = "all",
        help = "Memory ID to delete (omit when using --all)"
    )]
    pub id: Option<String>,
    #[arg(
        long,
        conflicts_with = "id",
        help = "Delete all memories matching scope filters"
    )]
    pub all: bool,
    #[arg(long, help = "Show what would be deleted without deleting")]
    pub dry_run: bool,
    #[arg(long, help = "Skip confirmation")]
    pub force: bool,
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    #[command(about = "Store a value in the config file")]
    Set { key: ConfigKey, value: String },
    #[command(about = "Show the config file path and values")]
    Show,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ConfigKey {
    Url,
    ApiKey,
    UserId,
}
