use anyhow::{Result, bail};
use async_trait::async_trait;

use crate::protocol::{
    GitBranch, RepositoryGitInspectParams, RepositoryGitState, WorkspaceBaseRequest,
};
use crate::rpc::{RpcClient, RpcClientError};

use super::super::prompt::{Choice, Interaction};
use super::super::status::natural_cmp;
use super::{CreateWalkthrough, WorktreeChoice, has_explicit_code_base, select_workspace};

#[async_trait]
pub(super) trait GitInspector: Send + Sync {
    async fn inspect(&self, params: RepositoryGitInspectParams) -> Result<RepositoryGitState>;
}

#[async_trait]
impl GitInspector for RpcClient {
    async fn inspect(&self, params: RepositoryGitInspectParams) -> Result<RepositoryGitState> {
        match self.request(params).await {
            Ok(state) => Ok(state),
            Err(RpcClientError::Remote(error)) if error.code == "METHOD_NOT_FOUND" => {
                bail!("Git selection needs a matching cocod version; restart cocod after updating")
            }
            Err(error) => Err(error.into()),
        }
    }
}

impl CreateWalkthrough<'_> {
    pub(super) async fn prompt_worktree(&mut self) -> Result<()> {
        if self.args.branch.is_some() || self.args.checkout.is_some() || self.args.detached {
            return Ok(());
        }
        let name = self.args.name.as_ref().unwrap();
        let mut choices = vec![
            Choice::new("New branch", Some(name.clone())).with_default_marker(),
            Choice::new("Custom branch", None),
        ];
        let mut actions = vec![WorktreeChoice::DefaultBranch, WorktreeChoice::NamedBranch];
        if !has_explicit_code_base(&self.args) {
            choices.push(Choice::new(
                "Existing branch",
                Some("use the branch itself".into()),
            ));
            actions.push(WorktreeChoice::ExistingBranch);
        }
        choices.push(Choice::new("Detached HEAD", None));
        actions.push(WorktreeChoice::Detached);
        loop {
            match actions[self.interaction.select("Worktree", &choices)?] {
                WorktreeChoice::DefaultBranch => return Ok(()),
                WorktreeChoice::NamedBranch => {
                    self.args.branch = Some(self.interaction.text("Branch name")?);
                    return Ok(());
                }
                WorktreeChoice::Detached => {
                    self.args.detached = true;
                    return Ok(());
                }
                WorktreeChoice::ExistingBranch => {
                    if let Some(branch) = self.select_branch(true).await? {
                        self.args.checkout = Some(branch.name);
                        return Ok(());
                    }
                }
            }
        }
    }

    pub(super) async fn prompt_code_base(&mut self) -> Result<()> {
        if self.args.checkout.is_some() || has_explicit_code_base(&self.args) {
            return Ok(());
        }
        loop {
            let mut choices = vec![
                Choice::new("Branch", None),
                Choice::new("Workspace", None),
                Choice::new("Other revision", Some("tag, commit, or expression".into())),
            ];
            let has_head = self.source.head_sha.is_some();
            if has_head {
                choices.insert(
                    0,
                    Choice::new("Current checkout", Some(checkout_label(&self.source)))
                        .with_default_marker(),
                );
            }
            let selected = self.interaction.select("Code", &choices)?;
            if has_head && selected == 0 {
                return Ok(());
            }
            match selected - usize::from(has_head) {
                0 => {
                    if let Some(branch) = self.select_branch(false).await? {
                        self.args.base = Some(branch.reference);
                        return Ok(());
                    }
                }
                1 => {
                    let mut workspaces = self.list_workspaces(None).await?;
                    workspaces.retain(|item| item.workspace.worktree_path.is_some());
                    if workspaces.is_empty() {
                        self.interaction
                            .notice("No workspace with reusable code found.")?;
                        continue;
                    }
                    let selected = select_workspace(
                        &mut self.interaction,
                        "Code from workspace",
                        &mut workspaces,
                        self.args.sort,
                    )?;
                    self.args.base_workspace = Some(selected.id);
                    self.selected_code_workspace = Some(selected.name);
                    return Ok(());
                }
                2 => {
                    self.args.base = Some(self.interaction.text("Git revision")?);
                    return Ok(());
                }
                _ => unreachable!("validated selection"),
            }
        }
    }

    async fn select_branch(&mut self, checkout: bool) -> Result<Option<GitBranch>> {
        let state = self
            .inspector
            .inspect(RepositoryGitInspectParams {
                path: self.repository_path.clone(),
                base: None,
                include_branches: true,
            })
            .await?;
        let mut branches: Vec<_> = state
            .branches
            .into_iter()
            .filter(|branch| !checkout || (!branch.remote && branch.checkout_path.is_none()))
            .collect();
        branches.sort_by(|a, b| {
            a.remote
                .cmp(&b.remote)
                .then_with(|| {
                    (Some(&b.name) == state.branch.as_ref())
                        .cmp(&(Some(&a.name) == state.branch.as_ref()))
                })
                .then_with(|| natural_cmp(&a.name, &b.name))
        });
        if branches.is_empty() {
            self.interaction.notice(if checkout {
                "No free local branch found. Create a new branch instead."
            } else {
                "No branch found. Choose another code source."
            })?;
            return Ok(None);
        }
        let choices: Vec<_> = branches
            .iter()
            .map(|branch| {
                let mut detail = vec![short_sha(&branch.commit).to_owned()];
                if branch.remote {
                    detail.push("remote".into());
                } else if Some(&branch.name) == state.branch.as_ref() {
                    detail.push("current".into());
                }
                Choice::new(&branch.name, Some(detail.join(" · ")))
            })
            .collect();
        let selected = self.interaction.select(
            if checkout {
                "Use branch"
            } else {
                "Code from branch"
            },
            &choices,
        )?;
        Ok(Some(branches.remove(selected)))
    }

    pub(super) fn prompt_local_changes(&mut self) -> Result<()> {
        if self.changes_preset || !self.uses_current_checkout() {
            return Ok(());
        }
        self.changes_prompted = Some(self.source.changes.clone());
        self.args.carry_changes = false;
        self.args.carry_untracked = false;
        self.args.dirty = false;
        let changes = &self.source.changes;
        if changes.tracked_files == 0 && changes.untracked_files == 0 {
            return Ok(());
        }
        let mut choices = vec![Choice::new("Don't copy", None).with_default_marker()];
        let carries_untracked = changes.untracked_files > 0;
        choices.push(if carries_untracked {
            Choice::new(
                "Copy tracked + untracked",
                Some(format!(
                    "{} tracked · {} untracked",
                    changes.tracked_files, changes.untracked_files
                )),
            )
        } else {
            Choice::new(
                "Copy tracked",
                Some(format!(
                    "{} {}",
                    changes.tracked_files,
                    if changes.tracked_files == 1 {
                        "file"
                    } else {
                        "files"
                    }
                )),
            )
        });
        if self.interaction.select("Git changes", &choices)? == 1 {
            self.args.carry_changes = true;
            self.args.carry_untracked = carries_untracked;
        }
        Ok(())
    }

    fn uses_current_checkout(&self) -> bool {
        self.args.checkout.is_none() && !has_explicit_code_base(&self.args)
    }

    pub(super) async fn prepare_git_review(&mut self) -> Result<()> {
        let base = if let Some(branch) = &self.args.checkout {
            WorkspaceBaseRequest::Revision {
                revision: format!("refs/heads/{branch}"),
            }
        } else if let Some(workspace) = self
            .args
            .base_workspace
            .as_ref()
            .or(self.args.fork_from.as_ref())
        {
            WorkspaceBaseRequest::Workspace {
                workspace: workspace.clone(),
            }
        } else {
            WorkspaceBaseRequest::Revision {
                revision: self.args.base.clone().unwrap_or_else(|| "HEAD".into()),
            }
        };
        let state = self
            .inspector
            .inspect(RepositoryGitInspectParams {
                path: self.repository_path.clone(),
                base: Some(base.clone()),
                include_branches: false,
            })
            .await?;
        if state.repository_id != self.source.repository_id {
            bail!("the selected repository changed; run create again");
        }
        let commit = state
            .base_sha
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Git inspection returned no base commit"))?;
        let label = if self.uses_current_checkout() {
            checkout_label(&state)
        } else {
            let name = match &base {
                WorkspaceBaseRequest::Workspace { workspace } => format!(
                    "workspace {}",
                    self.selected_code_workspace.as_deref().unwrap_or(workspace)
                ),
                WorkspaceBaseRequest::Revision { revision } => revision
                    .strip_prefix("refs/heads/")
                    .or_else(|| revision.strip_prefix("refs/remotes/"))
                    .unwrap_or(revision)
                    .to_owned(),
            };
            format!("{name} · {}", short_sha(&commit))
        };
        if self.uses_current_checkout() && self.source.head_sha != state.head_sha {
            self.interaction
                .notice("Current checkout changed. Review the updated commit.")?;
        }
        let changed = self
            .changes_prompted
            .as_ref()
            .is_some_and(|previous| *previous != state.changes);
        self.source = state;
        if changed {
            self.interaction
                .notice("Local files changed. Review what to copy.")?;
            self.prompt_local_changes()?;
        }
        self.reviewed_code = Some(label);
        self.args.expected_base_sha = Some(commit);
        Ok(())
    }
}

fn short_sha(sha: &str) -> &str {
    sha.get(..8).unwrap_or(sha)
}

fn checkout_label(state: &RepositoryGitState) -> String {
    format!(
        "{} · {}",
        state.branch.as_deref().unwrap_or("Detached HEAD"),
        state
            .head_sha
            .as_deref()
            .map(short_sha)
            .unwrap_or("no commits")
    )
}
