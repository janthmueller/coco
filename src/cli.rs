use anyhow::Result;
use clap::Parser;

mod args;
mod commands;
mod jump;
mod output;
mod status;

#[cfg(test)]
mod tests;

pub use args::Cli;

pub async fn run_from_env() -> Result<()> {
    commands::run(Cli::parse()).await
}
