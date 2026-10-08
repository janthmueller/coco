use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::domain::CodexModel;
use crate::paths::CocoPaths;
use crate::protocol::{ModelListParams, RepositoryScope, WorkspaceListItem, WorkspaceListParams};
use crate::rpc::{RpcClient, RpcClientError};

use super::args::{CreateArgs, StatusSort};
use super::output::phase_label;
use super::prompt::{Choice, Interaction, ReviewField};
use super::status::sort_workspace_collection;

pub(super) fn walkthrough_requested(args: &CreateArgs) -> bool {
    args.interactive || args.name.is_none()
}

pub(super) async fn walkthrough(
    paths: &CocoPaths,
    repository_path: PathBuf,
    args: CreateArgs,
    interaction: &mut dyn Interaction,
) -> Result<CreateArgs> {
    validate_local_change_flags(args.carry_changes, args.carry_untracked, args.dirty)?;
    let (args, selected_context_source) =
        resolve_current_context_selection(paths, &repository_path, args).await?;
    let client = RpcClient::new(paths.socket_path.clone());
    CreateWalkthrough {
        args,
        repository_path,
        worktrees_root: paths.worktrees_dir.clone(),
        client,
        interaction,
        selected_code_workspace: None,
        selected_context_source,
    }
    .run()
    .await
}

pub(super) async fn resolve_current_context(
    paths: &CocoPaths,
    repository_path: &Path,
    args: CreateArgs,
) -> Result<CreateArgs> {
    resolve_current_context_selection(paths, repository_path, args)
        .await
        .map(|(args, _)| args)
}

async fn resolve_current_context_selection(
    paths: &CocoPaths,
    repository_path: &Path,
    mut args: CreateArgs,
) -> Result<(CreateArgs, Option<String>)> {
    if args.context.as_deref() != Some(".") {
        return Ok((args, None));
    }
    if !path_is_within(repository_path, &paths.worktrees_dir) {
        bail!(
            "--context . requires the selected path to be inside an open CoCo workspace worktree"
        );
    }

    let client = RpcClient::new(paths.socket_path.clone());
    let workspaces = list_repository_workspaces(&client, repository_path, None).await?;
    let current = workspace_owning_path(&workspaces, repository_path).ok_or_else(|| {
        anyhow::anyhow!(
            "--context . could not find an open CoCo workspace for {}",
            repository_path.display()
        )
    })?;
    args.context = Some(format!("workspace:{}", current.workspace.id));
    Ok((args, Some(format!("workspace {}", current.workspace.name))))
}

struct CreateWalkthrough<'a> {
    args: CreateArgs,
    repository_path: PathBuf,
    worktrees_root: PathBuf,
    client: RpcClient,
    interaction: &'a mut dyn Interaction,
    selected_code_workspace: Option<String>,
    selected_context_source: Option<String>,
}

impl CreateWalkthrough<'_> {
    async fn run(mut self) -> Result<CreateArgs> {
        self.prompt_name()?;
        self.prompt_worktree()?;
        self.prompt_code_base().await?;
        self.prompt_local_changes()?;
        self.prompt_context().await?;
        self.prompt_profile()?;
        self.prompt_model().await?;
        self.prompt_post_action()?;
        self.confirm_creation()?;
        Ok(self.args)
    }

    fn prompt_name(&mut self) -> Result<()> {
        if self.args.name.is_none() {
            self.args.name = Some(self.interaction.text("Workspace name")?);
        }
        Ok(())
    }

    fn prompt_worktree(&mut self) -> Result<()> {
        if self.args.branch.is_some() || self.args.checkout.is_some() || self.args.detached {
            return Ok(());
        }
        let name = self
            .args
            .name
            .as_deref()
            .expect("the walkthrough resolves the workspace name first");
        let default_branch = format!("coco/{name}");
        let mut choices = vec![
            Choice::new("New branch", Some(default_branch)).with_default_marker(),
            Choice::new("Custom branch", None),
        ];
        let mut actions = vec![WorktreeChoice::DefaultBranch, WorktreeChoice::NamedBranch];
        if !has_explicit_code_base(&self.args) {
            choices.push(Choice::new("Existing branch", None));
            actions.push(WorktreeChoice::ExistingBranch);
        }
        choices.push(Choice::new("Detached HEAD", None));
        actions.push(WorktreeChoice::Detached);

        let selected = self.interaction.select("Worktree", &choices)?;
        match actions[selected] {
            WorktreeChoice::DefaultBranch => {}
            WorktreeChoice::NamedBranch => {
                self.args.branch = Some(self.interaction.text("Branch name")?);
            }
            WorktreeChoice::ExistingBranch => {
                self.args.checkout = Some(self.interaction.text("Existing branch")?);
            }
            WorktreeChoice::Detached => self.args.detached = true,
        }
        Ok(())
    }

    async fn prompt_code_base(&mut self) -> Result<()> {
        if self.args.checkout.is_some()
            || self.args.base.is_some()
            || self.args.base_workspace.is_some()
            || self.args.fork_from.is_some()
        {
            return Ok(());
        }
        loop {
            let choices = [
                Choice::new("Current HEAD", None).with_default_marker(),
                Choice::new("Workspace", None),
                Choice::new("Git revision", Some("branch, tag, or commit".to_owned())),
            ];
            match self.interaction.select("Code", &choices)? {
                0 => return Ok(()),
                1 => {
                    let mut workspaces = self.list_workspaces(None).await?;
                    workspaces.retain(|item| item.workspace.worktree_path.is_some());
                    if workspaces.is_empty() {
                        self.interaction
                            .notice("No workspace with reusable code found.")?;
                        continue;
                    }
                    let selected =
                        select_workspace(self.interaction, "Code from workspace", &workspaces)?;
                    self.args.base_workspace = Some(selected.id);
                    self.selected_code_workspace = Some(selected.name);
                    return Ok(());
                }
                2 => {
                    self.args.base = Some(self.interaction.text("Git revision")?);
                    return Ok(());
                }
                _ => unreachable!("the picker returns an index within its choices"),
            }
        }
    }

    fn prompt_local_changes(&mut self) -> Result<()> {
        let explicit = self.args.carry_changes || self.args.carry_untracked || self.args.dirty;
        let uses_invoking_head = self.args.checkout.is_none()
            && self.args.base.is_none()
            && self.args.base_workspace.is_none()
            && self.args.fork_from.is_none();
        if explicit || !uses_invoking_head {
            return Ok(());
        }
        let choices = [
            Choice::new("Don't copy", None).with_default_marker(),
            Choice::new(
                "Copy tracked",
                Some("fails if non-ignored untracked files exist".to_owned()),
            ),
            Choice::new("Copy tracked + untracked", None),
        ];
        match self.interaction.select("Git changes", &choices)? {
            0 => {}
            1 => self.args.carry_changes = true,
            2 => self.args.dirty = true,
            _ => unreachable!("the picker returns an index within its choices"),
        }
        Ok(())
    }

    async fn prompt_context(&mut self) -> Result<()> {
        if self.args.context.is_some() || self.args.fork_from.is_some() {
            return Ok(());
        }
        let compact_was_requested = self.args.compact_context;
        let mut workspaces = if path_is_within(&self.repository_path, &self.worktrees_root) {
            self.list_workspaces(Some(reusable_context_phases()))
                .await?
        } else {
            Vec::new()
        };
        let current = workspace_owning_path(&workspaces, &self.repository_path)
            .map(SelectedWorkspace::from_item);
        let has_current = current.is_some();
        if let Some(current) = &current {
            workspaces.retain(|item| item.workspace.id != current.id);
        }

        let mut choices = Vec::with_capacity(4);
        let mut actions = Vec::with_capacity(4);
        if !compact_was_requested {
            choices.push(Choice::new("Fresh", None).with_default_marker());
            actions.push(ContextChoice::Fresh);
        }
        if let Some(current) = current {
            choices.push(Choice::new("Current workspace", Some(current.name.clone())));
            actions.push(ContextChoice::Current(current));
        }
        choices.push(Choice::new(
            if has_current {
                "Another workspace"
            } else {
                "Workspace"
            },
            None,
        ));
        actions.push(ContextChoice::Existing);
        choices.push(Choice::new("Thread ID", None));
        actions.push(ContextChoice::Thread);

        loop {
            let selected = self.interaction.select("Context", &choices)?;
            match &actions[selected] {
                ContextChoice::Fresh => return Ok(()),
                ContextChoice::Current(current) => {
                    self.args.context = Some(format!("workspace:{}", current.id));
                    self.selected_context_source = Some(format!("workspace {}", current.name));
                    break;
                }
                ContextChoice::Existing => {
                    if workspaces.is_empty() {
                        workspaces = self
                            .list_workspaces(Some(reusable_context_phases()))
                            .await?;
                    }
                    if let Some(current) = workspace_owning_path(&workspaces, &self.repository_path)
                    {
                        let current_id = current.workspace.id.clone();
                        workspaces.retain(|item| item.workspace.id != current_id);
                    }
                    if workspaces.is_empty() {
                        let qualifier = if has_current { "other " } else { "" };
                        self.interaction.notice(&format!(
                            "No {qualifier}workspace with reusable context found."
                        ))?;
                        continue;
                    }
                    let selected =
                        select_workspace(self.interaction, "Context from workspace", &workspaces)?;
                    self.args.context = Some(format!("workspace:{}", selected.id));
                    self.selected_context_source = Some(format!("workspace {}", selected.name));
                    break;
                }
                ContextChoice::Thread => {
                    let thread_id = self.interaction.text("Codex thread ID")?;
                    let context = if thread_id.starts_with("thread:") {
                        thread_id
                    } else {
                        format!("thread:{thread_id}")
                    };
                    self.selected_context_source = Some(context_reference_label(&context));
                    self.args.context = Some(context);
                    break;
                }
            }
        }
        if !compact_was_requested {
            let copy_choices = [
                Choice::new("Full context", None).with_default_marker(),
                Choice::new("Compact context", None),
            ];
            self.args.compact_context =
                self.interaction.select("Context size", &copy_choices)? == 1;
        }
        Ok(())
    }

    fn prompt_profile(&mut self) -> Result<()> {
        if self.args.profile.is_some() {
            return Ok(());
        }
        let choices = [
            Choice::new("Use Codex config", None).with_default_marker(),
            Choice::new("Choose profile", None),
        ];
        if self.interaction.select("Profile", &choices)? == 1 {
            self.args.profile = Some(self.interaction.text("Profile name")?);
        }
        Ok(())
    }

    async fn prompt_model(&mut self) -> Result<()> {
        if self.args.model.is_some() {
            return Ok(());
        }
        loop {
            let choices = [
                Choice::new("Use configured model", None).with_default_marker(),
                Choice::new("Choose model", None),
            ];
            if self.interaction.select("Model", &choices)? == 0 {
                return Ok(());
            }
            let models: Vec<CodexModel> = self.client.request(ModelListParams {}).await?;
            if models.is_empty() {
                self.interaction.notice(
                    "No selectable Codex models found. Use the configured model or try again later.",
                )?;
                continue;
            }
            let model_choices = models.iter().map(model_choice).collect::<Vec<_>>();
            let selected = self.interaction.select("Choose model", &model_choices)?;
            self.args.model = Some(models[selected].model.clone());
            return Ok(());
        }
    }

    fn prompt_post_action(&mut self) -> Result<()> {
        if self.args.send.is_some() || self.args.jump {
            return Ok(());
        }
        let choices = [
            Choice::new("Return to shell", None).with_default_marker(),
            Choice::new("Send message", None),
            Choice::new("Open Codex", None),
            Choice::new("Send, then open Codex", None),
        ];
        match self.interaction.select("After create", &choices)? {
            0 => {}
            1 => self.args.send = Some(self.interaction.text("Message")?),
            2 => self.args.jump = true,
            3 => {
                self.args.send = Some(self.interaction.text("Message")?);
                self.args.jump = true;
            }
            _ => unreachable!("the picker returns an index within its choices"),
        }
        Ok(())
    }

    fn confirm_creation(&mut self) -> Result<()> {
        let fields = create_review_fields(
            &self.args,
            self.selected_code_workspace.as_deref(),
            self.selected_context_source.as_deref(),
        );
        let choices = [Choice::new("Create", None), Choice::new("Cancel", None)];
        if self
            .interaction
            .select_with_review("Create workspace", &fields, &choices)?
            == 0
        {
            Ok(())
        } else {
            bail!("workspace creation cancelled")
        }
    }

    async fn list_workspaces(&self, phases: Option<Vec<String>>) -> Result<Vec<WorkspaceListItem>> {
        let mut workspaces =
            list_repository_workspaces(&self.client, &self.repository_path, phases).await?;
        sort_workspace_collection(&mut workspaces, StatusSort::Name);
        Ok(workspaces)
    }
}

fn select_workspace(
    interaction: &mut dyn Interaction,
    title: &str,
    workspaces: &[WorkspaceListItem],
) -> Result<SelectedWorkspace> {
    debug_assert!(!workspaces.is_empty());
    let choices = workspaces.iter().map(workspace_choice).collect::<Vec<_>>();
    let selected = interaction.select(title, &choices)?;
    Ok(SelectedWorkspace {
        id: workspaces[selected].workspace.id.clone(),
        name: workspaces[selected].workspace.name.clone(),
    })
}

struct SelectedWorkspace {
    id: String,
    name: String,
}

impl SelectedWorkspace {
    fn from_item(item: &WorkspaceListItem) -> Self {
        Self {
            id: item.workspace.id.clone(),
            name: item.workspace.name.clone(),
        }
    }
}

enum ContextChoice {
    Fresh,
    Current(SelectedWorkspace),
    Existing,
    Thread,
}

#[derive(Clone, Copy)]
enum WorktreeChoice {
    DefaultBranch,
    NamedBranch,
    ExistingBranch,
    Detached,
}

fn has_explicit_code_base(args: &CreateArgs) -> bool {
    args.base.is_some() || args.base_workspace.is_some() || args.fork_from.is_some()
}

pub(super) fn validate_local_change_flags(
    carry_changes: bool,
    carry_untracked: bool,
    dirty: bool,
) -> Result<()> {
    if carry_untracked && !carry_changes && !dirty {
        bail!("--carry-untracked requires --carry-changes or --dirty");
    }
    Ok(())
}

fn reusable_context_phases() -> Vec<String> {
    [
        "idle",
        "not_loaded",
        "active",
        "waiting_for_approval",
        "waiting_for_input",
    ]
    .map(ToOwned::to_owned)
    .to_vec()
}

async fn list_repository_workspaces(
    client: &RpcClient,
    repository_path: &Path,
    phases: Option<Vec<String>>,
) -> Result<Vec<WorkspaceListItem>> {
    match client
        .request(WorkspaceListParams {
            scope: RepositoryScope::repository(repository_path.to_path_buf()),
            phases,
            include_resources: false,
            include_activity: false,
        })
        .await
    {
        Ok(workspaces) => Ok(workspaces),
        Err(RpcClientError::Remote(error)) if error.code == "REPOSITORY_NOT_REGISTERED" => {
            Ok(Vec::new())
        }
        Err(error) => Err(error.into()),
    }
}

fn workspace_owning_path<'a>(
    workspaces: &'a [WorkspaceListItem],
    path: &Path,
) -> Option<&'a WorkspaceListItem> {
    let path = normalize_path(path);
    workspaces
        .iter()
        .filter_map(|item| {
            let worktree = item.workspace.worktree_path.as_deref()?;
            let worktree = normalize_path(worktree);
            path.starts_with(&worktree)
                .then_some((worktree.components().count(), item))
        })
        .max_by_key(|(depth, _)| *depth)
        .map(|(_, item)| item)
}

fn path_is_within(path: &Path, parent: &Path) -> bool {
    normalize_path(path).starts_with(normalize_path(parent))
}

fn normalize_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn workspace_choice(item: &WorkspaceListItem) -> Choice {
    let branch = item.workspace.branch_name.as_deref().unwrap_or("detached");
    Choice::new(
        item.workspace.name.clone(),
        Some(format!(
            "{} · {branch}",
            phase_label(item.workspace.phase.as_str())
        )),
    )
}

fn model_choice(model: &CodexModel) -> Choice {
    let detail = (model.display_name != model.model).then(|| model.model.clone());
    let choice = Choice::new(model.display_name.clone(), detail);
    if model.is_default {
        choice.with_default_marker()
    } else {
        choice
    }
}

fn context_reference_label(reference: &str) -> String {
    if let Some(thread_id) = reference.strip_prefix("thread:") {
        format!("thread {thread_id}")
    } else if let Some(workspace) = reference.strip_prefix("workspace:") {
        format!("workspace {workspace}")
    } else {
        reference.to_owned()
    }
}

fn create_review_fields(
    args: &CreateArgs,
    selected_code_workspace: Option<&str>,
    selected_context_source: Option<&str>,
) -> Vec<ReviewField> {
    let name = args
        .name
        .as_deref()
        .expect("the walkthrough resolves the workspace name first");
    let binding = if let Some(branch) = &args.checkout {
        format!("checkout {branch}")
    } else if args.detached {
        "detached".to_owned()
    } else {
        format!(
            "branch {}",
            args.branch
                .clone()
                .unwrap_or_else(|| format!("coco/{name}"))
        )
    };
    let code = if let Some(workspace) = selected_code_workspace {
        format!("workspace {workspace}")
    } else if let Some(workspace) = &args.base_workspace {
        format!("workspace {workspace}")
    } else if let Some(revision) = &args.base {
        revision.clone()
    } else if let Some(workspace) = &args.fork_from {
        format!("workspace {workspace}")
    } else if let Some(branch) = &args.checkout {
        format!("branch {branch}")
    } else {
        "HEAD".to_owned()
    };
    let context_source = selected_context_source
        .map(str::to_owned)
        .or_else(|| args.context.as_deref().map(context_reference_label))
        .or_else(|| {
            args.fork_from
                .as_deref()
                .map(|source| format!("workspace {source}"))
        });
    let context = if let Some(source) = context_source {
        if args.compact_context {
            format!("{source} · compact")
        } else {
            source
        }
    } else {
        "fresh".to_owned()
    };
    let changes = if args.dirty || args.carry_untracked {
        "copy tracked + untracked"
    } else if args.carry_changes {
        "copy tracked"
    } else {
        "don't copy"
    };
    let profile = args.profile.as_deref().unwrap_or("Codex config");
    let model = args.model.as_deref().unwrap_or("configured model");
    let action = match (args.send.is_some(), args.jump) {
        (false, false) => "return to shell",
        (true, false) => "send message",
        (false, true) => "open Codex",
        (true, true) => "send, then open Codex",
    };
    vec![
        ReviewField::new("Workspace", name),
        ReviewField::new("Worktree", binding),
        ReviewField::new("Code", code),
        ReviewField::new("Context", context),
        ReviewField::new("Git changes", changes),
        ReviewField::new("Profile", profile),
        ReviewField::new("Model", model),
        ReviewField::new("After create", action),
    ]
}

#[cfg(test)]
mod tests;
