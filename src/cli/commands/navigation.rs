use std::io::{self, Write};

use crate::domain::WorktreeMode;
use crate::protocol::{WorkspacePathEntry, WorkspacePathGetParams, WorkspacePathListParams};

use super::*;

pub(super) async fn run_scoped(
    command: Command,
    paths: &CocoPaths,
    repository_path: &Path,
    has_scope_path: bool,
    all_repos: bool,
    global: bool,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    let args = match command {
        Command::Path(args) => args,
        Command::Cd(_) => {
            reject_all_repos_for_reference(all_repos, "cd")?;
            bail!(
                "coco cd needs shell integration. Enable it in your shell:\n\
                 Bash: eval \"$(coco shell init bash)\"\n\
                 Zsh:  eval \"$(coco shell init zsh)\"\n\
                 Fish: coco shell init fish | source\n\
                 Without integration: cd \"$(coco path <WORKSPACE>)\""
            );
        }
        _ => unreachable!("non-navigation command"),
    };
    reject_all_repos_for_reference(all_repos, "path")?;
    run(
        paths,
        // Directory navigation is metadata-only: do not fetch native recency
        // merely to share the target commands' selection descriptor.
        workspace_selection(
            repository_path,
            has_scope_path,
            args.workspace,
            global,
            WorkspaceSort::Name,
        ),
        interaction,
    )
    .await
}

async fn run(
    paths: &CocoPaths,
    selection: WorkspaceSelection,
    interaction: &mut dyn Interaction,
) -> Result<()> {
    let path = resolve_path(paths, selection, interaction).await?;
    let path = printable_path(&path)?;
    writeln!(io::stdout().lock(), "{path}").context("could not write workspace path")
}

async fn resolve_path(
    paths: &CocoPaths,
    selection: WorkspaceSelection,
    interaction: &mut dyn Interaction,
) -> Result<PathBuf> {
    let client = RpcClient::new(paths.socket_path.clone());
    let WorkspaceSelection {
        repository_scope,
        has_explicit_path,
        workspace,
        global,
        sort: _,
    } = selection;
    let (scope, workspace) = if let Some(workspace) = workspace {
        (
            scope_for_reference(repository_scope, &workspace, global),
            workspace,
        )
    } else {
        require_interactive(interaction, "workspace")?;
        let scope = if global {
            RepositoryScope::AllRepositories
        } else {
            let RepositoryScope::Repository { path } = repository_scope else {
                unreachable!("non-global navigation begins with a repository scope")
            };
            RepositoryScope::repository(
                resolve_interactive_repository(paths, path, has_explicit_path, interaction).await?,
            )
        };
        let mut entries = client
            .request(WorkspacePathListParams {
                scope: scope.clone(),
            })
            .await?;
        if entries.is_empty() {
            bail!("no open worktrees are available; use `coco create` or `coco reopen`");
        }
        sort_entries(&mut entries);
        let choices = entries
            .iter()
            .map(|entry| choice(entry, global))
            .collect::<Vec<_>>();
        let index = interaction.select("Workspace directory", &choices)?;
        (scope, entries[index].workspace_id.clone())
    };
    // Resolve the selected UUID again: a concurrent close/delete must not
    // make us return a directory from the earlier picker snapshot.
    Ok(client
        .request(WorkspacePathGetParams { scope, workspace })
        .await?)
}

fn sort_entries(entries: &mut [WorkspacePathEntry]) {
    entries.sort_by(|left, right| {
        super::super::status::natural_cmp(&left.name, &right.name)
            .then_with(|| left.repository.root_path.cmp(&right.repository.root_path))
            .then_with(|| left.workspace_id.cmp(&right.workspace_id))
    });
}

fn choice(entry: &WorkspacePathEntry, global: bool) -> Choice {
    let mut detail = Vec::new();
    if let Some(branch) = entry
        .branch_name
        .as_ref()
        .filter(|branch| **branch != entry.name)
    {
        detail.push(branch.clone());
    } else if entry.worktree_mode == WorktreeMode::Detached {
        detail.push("detached".to_owned());
    }
    if global {
        detail.push(entry.repository.root_path.display().to_string());
    }
    Choice::new(
        entry.name.clone(),
        (!detail.is_empty()).then(|| detail.join(" · ")),
    )
}

fn printable_path(path: &Path) -> Result<&str> {
    let value = path.to_str().context("workspace path is not valid UTF-8")?;
    if !path.is_absolute() || value.chars().any(char::is_control) {
        bail!("workspace path must be absolute and contain no control characters");
    }
    Ok(value)
}

#[cfg(test)]
mod tests;
