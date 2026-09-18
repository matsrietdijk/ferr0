mod cli;
mod client;
mod config;
mod input;
mod output;
mod setup;

use std::{path::Path, process::ExitCode};

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command, ConfigCommand, GlobalArgs, MemoryCommand};
use client::{Client, Message};

fn main() -> Result<ExitCode> {
    let cli = Cli::parse();
    let json = cli.global.json && matches!(cli.command, Command::Memory(_));
    match run(cli) {
        Err(error) if json => {
            println!("{}", output::pretty(&output::error(&error)));
            Ok(ExitCode::FAILURE)
        }
        result => result.map(|()| ExitCode::SUCCESS),
    }
}

fn run(cli: Cli) -> Result<()> {
    let path = config::default_path()?;
    match cli.command {
        Command::Memory(command) => memory_command(&path, cli.global, command),
        Command::Config(command) => config_command(&path, command),
        Command::Setup => setup::run(&path),
    }
}

fn memory_command(path: &Path, global: GlobalArgs, command: MemoryCommand) -> Result<()> {
    let json = global.json;
    let settings = config::resolve(global, config::load(path)?)?;
    let client = Client::new(&settings.url, settings.api_key)?;
    let scope = &settings.scope;
    let not_found = "No memories found.";
    let (response, empty) = match command {
        MemoryCommand::Add {
            text,
            messages,
            file,
        } => {
            let messages = match (messages, file) {
                (Some(json), _) => input::parse_messages(&json)?,
                (None, Some(file)) => input::messages_from_file(&file)?,
                (None, None) => vec![Message::user(input::from_arg_or_stdin(text, "text")?)],
            };
            (client.add(&messages, scope)?, "No memories added.")
        }
        MemoryCommand::Search { query, limit } => {
            let query = input::from_arg_or_stdin(query, "query")?;
            (client.search(&query, scope, limit)?, not_found)
        }
        MemoryCommand::List { limit } => (client.list(scope, limit)?, not_found),
        MemoryCommand::Get { id } => (client.get(&id)?, ""),
        MemoryCommand::Update { id, text } => (client.update(&id, &text)?, ""),
        MemoryCommand::Delete { id } => (client.delete(&id)?, ""),
    };
    if json {
        println!("{}", output::pretty(&response));
    } else {
        println!("{}", output::render(&response, empty));
    }
    Ok(())
}

fn config_command(path: &Path, command: ConfigCommand) -> Result<()> {
    let mut file = config::load(path)?;
    match command {
        ConfigCommand::Set { key, value } => {
            file.set(key, value);
            config::save(path, &file)?;
            println!("Saved to {}", path.display());
        }
        ConfigCommand::Show => println!("{}\n{}", path.display(), file.describe()),
    }
    Ok(())
}
