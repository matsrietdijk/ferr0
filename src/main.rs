mod cli;
mod client;
mod config;
mod confirm;
mod input;
mod output;
mod setup;

use std::{path::Path, process::ExitCode};

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command, ConfigCommand, DeleteArgs, GlobalArgs, MemoryCommand};
use client::{Client, Message, SERVER_LIST_LIMIT, Scope};
use serde_json::Value;

const DRY_RUN_NOTE: &str = "No changes made (dry run).";

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
        MemoryCommand::Delete(args) => return delete_command(&client, scope, json, args),
    };
    if json {
        println!("{}", output::pretty(&response));
    } else {
        println!("{}", output::render(&response, empty));
    }
    Ok(())
}

fn delete_command(client: &Client, scope: &Scope, json: bool, args: DeleteArgs) -> Result<()> {
    let DeleteArgs {
        id, dry_run, force, ..
    } = args;
    if let Some(id) = id {
        let response = if dry_run {
            client.get(&id)?
        } else {
            client.delete(&id)?
        };
        if json {
            println!("{}", output::pretty(&response));
        } else {
            println!("{}", output::render(&response, ""));
            if dry_run {
                println!("{DRY_RUN_NOTE}");
            }
        }
        return Ok(());
    }
    confirm::require_force_for_json(force, json)?;
    if dry_run {
        let response = client.list_deletable(scope)?;
        let count = response["results"].as_array().map_or(0, Vec::len);
        if count >= SERVER_LIST_LIMIT {
            eprintln!(
                "Counted the first {SERVER_LIST_LIMIT} memories; delete --all also deletes any others in the scope."
            );
        }
        if json {
            println!("{}", output::pretty(&response));
        } else {
            let noun = if count == 1 { "memory" } else { "memories" };
            println!("Would delete {count} {noun}.\n{DRY_RUN_NOTE}");
        }
        return Ok(());
    }
    let prompt = format!("Delete ALL memories for {scope}? This cannot be undone.");
    if !confirm::confirm(&prompt, force)? {
        println!("Cancelled.");
        return Ok(());
    }
    print_done(
        json,
        &client.delete_all(scope)?,
        "All matching memories deleted",
    )
}

fn print_done(json: bool, response: &Value, message: &str) -> Result<()> {
    if json {
        println!("{}", output::pretty(response));
    } else {
        println!("{message}");
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
