use std::io::{self, Write};

use anyhow::{Context, Result};
use clap::CommandFactory;

use super::args::{Cli, ShellCommand, ShellKind};

pub(super) fn run(command: &ShellCommand) -> Result<()> {
    let ShellCommand::Init { shell } = command;
    io::stdout()
        .lock()
        .write_all(script(*shell).as_bytes())
        .context("could not write shell initialization")
}

fn script(shell: ShellKind) -> String {
    // Derive the stopping points from Clap, so `coco send cd` can never be
    // mistaken for a directory change as commands or aliases are added.
    let cli = Cli::command();
    let commands = cli
        .get_subcommands()
        .flat_map(|command| std::iter::once(command.get_name()).chain(command.get_all_aliases()))
        .chain(std::iter::once("help"))
        .filter(|name| *name != "cd")
        .collect::<Vec<_>>();
    match shell {
        ShellKind::Bash | ShellKind::Zsh => include_str!("shell/posix.sh")
            .replace("@COMMANDS@", &commands.join(" "))
            .replace(
                "@SETUP@",
                if matches!(shell, ShellKind::Zsh) {
                    "emulate -L zsh"
                } else {
                    "# Bash integration"
                },
            ),
        ShellKind::Fish => {
            include_str!("shell/init.fish").replace("@COMMANDS@", &commands.join(" "))
        }
    }
}

#[cfg(test)]
mod tests;
