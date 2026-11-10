use anyhow::Result;
use clap::Parser;

mod ai;
mod args;
mod clients;
mod commands;
mod create;
mod decision;
mod doctor;
mod follow;
mod hooks;
mod jump;
mod output;
mod prompt;
mod shell;
mod signals;
mod status;
mod style;
mod turn;
mod workspace_picker;

#[cfg(test)]
mod tests;

pub use args::Cli;

pub async fn run_from_env() -> Result<std::process::ExitCode> {
    match commands::run(Cli::parse()).await {
        Err(error) if error.is::<doctor::ReportFailed>() => Ok(std::process::ExitCode::FAILURE),
        result => result.map(|_| std::process::ExitCode::SUCCESS),
    }
}
