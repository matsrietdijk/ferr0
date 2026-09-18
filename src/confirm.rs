use std::io::{self, IsTerminal};

use anyhow::{Result, bail};
use dialoguer::{Confirm, theme::ColorfulTheme};

pub fn require_force_for_json(force: bool, json: bool) -> Result<()> {
    if json && !force {
        bail!("Destructive operation requires --force in agent mode.");
    }
    Ok(())
}

pub fn confirm(prompt: &str, force: bool) -> Result<bool> {
    if force {
        return Ok(true);
    }
    require_terminal()?;
    let confirmed = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .default(false)
        .interact()?;
    Ok(confirmed)
}

fn require_terminal() -> Result<()> {
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        bail!("cannot ask for confirmation without a terminal: pass --force to confirm");
    }
    Ok(())
}
