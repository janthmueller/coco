use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

use super::commands::{overview_scope, reject_all_repos_for_reference, scope_for_reference};
use super::output::{print_json, safe_line, versioned};
use super::style::{Palette, Tone};
use crate::domain::signals::SignalPage;
use crate::paths::CocoPaths;
use crate::protocol::{RepositoryScope, SignalListParams, WorkspaceGetParams};
use crate::rpc::RpcClient;

#[cfg(test)]
mod tests;

#[derive(Debug, Subcommand)]
pub(super) enum SignalCommand {
    /// Read retained agent signals, or follow new signals until interrupted.
    #[command(visible_alias = "ls")]
    List(SignalListArgs),
}

impl SignalCommand {
    pub(super) fn all_repos(&self) -> bool {
        matches!(self, Self::List(args) if args.all_repos)
    }

    pub(super) fn global(&self) -> bool {
        matches!(self, Self::List(args) if args.global)
    }
}

#[derive(Debug, Args)]
pub(super) struct SignalListArgs {
    /// Workspace name or UUID. Omit it to read the selected repository.
    /// Use the UUID to read retained signals after a workspace was deleted.
    workspace: Option<String>,
    /// Read signals across all registered repositories.
    #[arg(short = 'a', long, conflicts_with = "workspace")]
    all_repos: bool,
    /// Resolve one named workspace across all registered repositories.
    #[arg(
        short = 'g',
        long,
        conflicts_with = "all_repos",
        requires = "workspace"
    )]
    global: bool,
    /// Filter by an exact signal name.
    #[arg(long)]
    name: Option<String>,
    /// Resume from nextCursor of a previous page with identical filters.
    #[arg(long, value_name = "CURSOR")]
    after: Option<String>,
    #[arg(long, default_value_t = 100, value_parser = clap::value_parser!(u32).range(1..=100))]
    limit: u32,
    /// Keep reading until Ctrl-C; does not start work or print Codex chat.
    #[arg(short = 'f', long)]
    follow: bool,
    /// Emit JSON; with --follow, one page per line, including its nextCursor.
    #[arg(long)]
    json: bool,
}

pub(super) async fn run(
    command: SignalCommand,
    paths: &CocoPaths,
    repository: PathBuf,
    all_repos: bool,
    global: bool,
) -> Result<()> {
    let client = RpcClient::new(paths.socket_path.clone());
    let SignalCommand::List(args) = command;
    let scope = list_scope(&args, repository, all_repos, global)?;
    list(args, &client, scope).await
}

fn list_scope(
    args: &SignalListArgs,
    repository: PathBuf,
    all_repos: bool,
    global: bool,
) -> Result<RepositoryScope> {
    let scope = RepositoryScope::repository(repository);
    if let Some(workspace) = &args.workspace {
        reject_all_repos_for_reference(all_repos, "signal list")?;
        return Ok(scope_for_reference(scope, workspace, global));
    }
    if global {
        bail!(
            "coco signal list --global requires a workspace name or ID; use --all-repos for an overview"
        );
    }
    Ok(overview_scope(scope, all_repos))
}

async fn list(args: SignalListArgs, client: &RpcClient, scope: RepositoryScope) -> Result<()> {
    let workspace_id = match args.workspace {
        Some(reference) if Uuid::parse_str(&reference).is_ok() => Some(reference),
        Some(reference) => Some(
            client
                .request(WorkspaceGetParams {
                    scope: scope.clone(),
                    workspace: reference,
                    include_resources: false,
                })
                .await?
                .workspace
                .id,
        ),
        None => None,
    };
    let all_repos = matches!(scope, RepositoryScope::AllRepositories);
    let mut request = SignalListParams {
        scope,
        workspace_id,
        name: args.name,
        after: args.after,
        limit: args.limit,
    };
    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);
    let mut first = true;
    loop {
        let page = tokio::select! {
            result = client.request(request.clone()) => result?,
            result = &mut interrupt, if args.follow => { result?; return Ok(()); }
        };
        let changed = request.after.as_deref() != Some(page.next_cursor.as_str());
        if first || !page.signals.is_empty() || (changed && args.json) {
            if args.json {
                print_json(versioned(serde_json::to_value(&page)?))?;
            } else {
                print_page(&page, all_repos, first);
            }
        }
        if !args.follow {
            if page.has_more && !args.json {
                eprintln!(
                    "More signals available. Continue with --after {}",
                    page.next_cursor
                );
            }
            return Ok(());
        }
        first = false;
        request.after = Some(page.next_cursor);
        if !page.has_more {
            tokio::select! {
                () = tokio::time::sleep(Duration::from_secs(1)) => {},
                result = &mut interrupt => { result?; return Ok(()); }
            }
        }
    }
}

fn print_page(page: &SignalPage, all_repos: bool, first: bool) {
    if page.signals.is_empty() && first {
        println!("No signals.");
    }
    let palette = Palette::stdout();
    for signal in &page.signals {
        let time = chrono::DateTime::from_timestamp_millis(signal.occurred_at_ms)
            .map(|time| time.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_default();
        let workspace = if all_repos {
            format!("{} / {}", signal.repository_name, signal.workspace_name)
        } else {
            signal.workspace_name.clone()
        };
        println!(
            "{}  {}  {}  {}",
            palette.paint(Tone::Dim, time),
            palette.paint(Tone::Bold, safe_line(&workspace)),
            palette.paint(
                Tone::CyanBold,
                format!("{}@{}", signal.name, signal.version)
            ),
            json!(signal.payload)
        );
    }
}
