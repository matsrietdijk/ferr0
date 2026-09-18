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
        #[arg(short, long, value_name = "JSON", help = "Metadata as JSON")]
        metadata: Option<String>,
        #[arg(long, help = "Store the messages as-is instead of extracting memories")]
        no_infer: bool,
        #[arg(
            long,
            value_name = "YYYY-MM-DD",
            help = "Expiration date, in the future"
        )]
        expires: Option<String>,
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
    #[command(about = "Change the text, metadata or expiration date of a memory")]
    Update {
        id: String,
        #[arg(help = "New memory text; read from stdin when omitted")]
        text: Option<String>,
        #[arg(short, long, value_name = "JSON", help = "Metadata as JSON")]
        metadata: Option<String>,
        #[arg(
            long,
            value_name = "YYYY-MM-DD",
            conflicts_with = "no_expires",
            help = "Expiration date, in the future"
        )]
        expires: Option<String>,
        #[arg(long, help = "Remove the expiration date")]
        no_expires: bool,
    },
    #[command(about = "Delete a memory")]
    Delete { id: String },
    #[command(about = "Add memories from a JSON file, one request per item")]
    Import {
        #[arg(help = "JSON file with the memories to add")]
        file: PathBuf,
    },
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

#[cfg(test)]
mod tests {
    use clap::error::ErrorKind;

    use super::*;

    #[test]
    fn update_rejects_expires_with_no_expires() {
        let error = Cli::try_parse_from([
            "ferr0",
            "update",
            "1",
            "--expires",
            "2026-12-31",
            "--no-expires",
        ])
        .err()
        .unwrap();
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
    }
}
