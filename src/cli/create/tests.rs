use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::Parser;
use serde_json::{Value, json};
use tempfile::tempdir;
use tokio::sync::watch;

use crate::domain::{
    ContextMode, ProfileSnapshot, Workspace, WorkspaceAvailability, WorkspaceLifecycle,
    WorkspacePhase, WorktreeMode,
};
use crate::paths::CocoPaths;
use crate::protocol::{
    RepositorySummary, WorkspaceBaseRequest, WorkspaceChangesRequest, WorkspaceContextRequest,
    WorkspaceContextSource, WorkspaceListItem, WorkspaceListParams, WorkspaceWorktreeRequest,
};
use crate::rpc::{RpcErrorPayload, RpcHandler, RpcServer};

use super::*;
use crate::cli::args::{Cli, Command};
use crate::cli::commands::normalize_create_args;

static RPC_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn defaults_prepare_a_fresh_workspace_without_contacting_discovery() {
    let args = create_args(&["coco", "create"]);
    let mut interaction = ScriptedInteraction::new([0, 0, 0, 0, 0, 0, 0, 0], ["review/api"]);

    let args = walkthrough(
        &test_paths(),
        PathBuf::from("/repo"),
        args,
        &mut interaction,
    )
    .await
    .unwrap();

    assert_eq!(args.name.as_deref(), Some("review/api"));
    assert_eq!(
        interaction.titles,
        [
            "Worktree",
            "Code",
            "Git changes",
            "Context",
            "Profile",
            "Model",
            "After create",
            "Create workspace",
        ]
    );
    assert_default_walkthrough_copy(&interaction);
    interaction.assert_consumed();

    let (params, message, jump) =
        normalize_create_args(PathBuf::from("/repo"), args, "operation".to_owned()).unwrap();
    assert_eq!(params.context, WorkspaceContextRequest::Fresh);
    assert_eq!(
        params.worktree,
        WorkspaceWorktreeRequest::NewBranch {
            branch: None,
            base: WorkspaceBaseRequest::Revision {
                revision: "HEAD".to_owned(),
            },
        }
    );
    assert_eq!(params.changes, WorkspaceChangesRequest::Reject);
    assert_eq!(params.profile, "default");
    assert!(message.is_none());
    assert!(!jump);
}

#[tokio::test]
async fn native_thread_context_can_be_compacted_before_send_and_jump() {
    let args = create_args(&["coco", "create", "review/api", "-i"]);
    let mut interaction = ScriptedInteraction::new(
        [0, 0, 0, 2, 1, 0, 0, 3, 0],
        ["0199-native-thread", "Review the API"],
    );

    let args = walkthrough(
        &test_paths(),
        PathBuf::from("/repo"),
        args,
        &mut interaction,
    )
    .await
    .unwrap();
    let confirmation = final_create_choice(&interaction);
    assert_eq!(confirmation, &Choice::new("Create", None));
    assert_eq!(final_review_value(&interaction, "Workspace"), "review/api");
    assert_eq!(
        final_review_value(&interaction, "Context"),
        "thread 0199-native-thread · compact"
    );
    assert_eq!(
        final_review_value(&interaction, "After create"),
        "send, then open Codex"
    );
    interaction.assert_consumed();

    let (params, message, jump) =
        normalize_create_args(PathBuf::from("/repo"), args, "operation".to_owned()).unwrap();
    assert_eq!(
        params.context,
        WorkspaceContextRequest::Fork {
            source: WorkspaceContextSource::Reference {
                reference: "thread:0199-native-thread".to_owned(),
            },
            compact: true,
        }
    );
    assert_eq!(message.as_deref(), Some("Review the API"));
    assert!(jump);
}

#[tokio::test]
async fn explicit_flags_seed_the_walkthrough_and_skip_resolved_steps() {
    let args = create_args(&[
        "coco",
        "create",
        "review/api",
        "-i",
        "--base",
        "main",
        "--context",
        "thread:0199-native-thread",
        "--compact-context",
        "--branch",
        "review/api",
        "--dirty",
        "--profile",
        "dev",
        "--model",
        "gpt-explicit",
        "--send",
        "Review it",
        "--jump",
    ]);
    let mut interaction = ScriptedInteraction::new([0], [] as [&str; 0]);

    let args = walkthrough(
        &test_paths(),
        PathBuf::from("/repo"),
        args,
        &mut interaction,
    )
    .await
    .unwrap();

    assert_eq!(interaction.titles, ["Create workspace"]);
    let confirmation = final_create_choice(&interaction);
    assert_eq!(confirmation, &Choice::new("Create", None));
    assert_eq!(final_review_value(&interaction, "Workspace"), "review/api");
    assert_eq!(
        final_review_value(&interaction, "Worktree"),
        "branch review/api"
    );
    assert_eq!(final_review_value(&interaction, "Code"), "main");
    assert_eq!(
        final_review_value(&interaction, "Context"),
        "thread 0199-native-thread · compact"
    );
    assert_eq!(
        final_review_value(&interaction, "Git changes"),
        "copy tracked + untracked"
    );
    assert_eq!(final_review_value(&interaction, "Profile"), "dev");
    assert_eq!(final_review_value(&interaction, "Model"), "gpt-explicit");
    assert_eq!(
        final_review_value(&interaction, "After create"),
        "send, then open Codex"
    );
    interaction.assert_consumed();
    assert_eq!(args.base.as_deref(), Some("main"));
    assert_eq!(args.context.as_deref(), Some("thread:0199-native-thread"));
    assert!(args.compact_context);
    assert_eq!(args.profile.as_deref(), Some("dev"));
    assert_eq!(args.model.as_deref(), Some("gpt-explicit"));
    assert_eq!(args.send.as_deref(), Some("Review it"));
    assert!(args.jump);
}

#[tokio::test]
async fn an_explicit_compaction_flag_requires_a_source_and_stays_enabled() {
    let args = create_args(&["coco", "create", "review/api", "-i", "-C"]);
    let mut interaction =
        ScriptedInteraction::new([0, 0, 0, 1, 0, 0, 0, 0], ["0199-native-thread"]);

    let args = walkthrough(
        &test_paths(),
        PathBuf::from("/repo"),
        args,
        &mut interaction,
    )
    .await
    .unwrap();
    interaction.assert_consumed();

    assert_eq!(args.context.as_deref(), Some("thread:0199-native-thread"));
    assert!(args.compact_context);
    let context_choices = choices_for_title(&interaction, "Context");
    assert!(context_choices.iter().all(|choice| !choice.is_default));
    assert!(
        !interaction
            .titles
            .iter()
            .any(|title| title == "Context size")
    );
}

#[tokio::test]
async fn empty_workspace_choices_explain_the_problem_and_return_to_the_walkthrough() {
    let _rpc_test_guard = RPC_TEST_LOCK.lock().await;
    let temporary = tempdir().unwrap();
    let paths = test_paths_at(temporary.path().to_path_buf());
    let handler = Arc::new(WorkspaceListHandler {
        seen_params: Arc::new(Mutex::new(Vec::new())),
        items: Vec::new(),
    });
    let server = RpcServer::bind(&paths.socket_path, handler).await.unwrap();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server_task = tokio::spawn(server.run(shutdown_rx));

    let args = create_args(&["coco", "create", "review/api", "-i"]);
    let mut interaction = ScriptedInteraction::new([0, 1, 0, 0, 1, 0, 0, 0, 0, 0], [] as [&str; 0]);
    let args = walkthrough(&paths, PathBuf::from("/repo"), args, &mut interaction)
        .await
        .unwrap();

    assert!(args.base.is_none());
    assert!(args.context.is_none());
    assert_eq!(
        interaction.notices,
        [
            "No workspace with reusable code found.",
            "No workspace with reusable context found.",
        ]
    );
    assert_eq!(
        interaction
            .titles
            .iter()
            .filter(|title| title.as_str() == "Code")
            .count(),
        2
    );
    assert_eq!(
        interaction
            .titles
            .iter()
            .filter(|title| title.as_str() == "Context")
            .count(),
        2
    );
    interaction.assert_consumed();

    shutdown_tx.send(true).unwrap();
    server_task.await.unwrap().unwrap();
}

#[tokio::test]
async fn unregistered_repository_choices_are_treated_as_empty_inventory() {
    let _rpc_test_guard = RPC_TEST_LOCK.lock().await;
    let temporary = tempdir().unwrap();
    let paths = test_paths_at(temporary.path().to_path_buf());
    let server = RpcServer::bind(&paths.socket_path, Arc::new(UnregisteredRepositoryHandler))
        .await
        .unwrap();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server_task = tokio::spawn(server.run(shutdown_rx));

    let args = create_args(&["coco", "create", "review/api", "-i"]);
    let mut interaction = ScriptedInteraction::new([0, 1, 0, 0, 1, 0, 0, 0, 0, 0], [] as [&str; 0]);
    let args = walkthrough(&paths, PathBuf::from("/repo"), args, &mut interaction)
        .await
        .unwrap();

    assert!(args.base.is_none());
    assert!(args.context.is_none());
    assert_eq!(
        interaction.notices,
        [
            "No workspace with reusable code found.",
            "No workspace with reusable context found.",
        ]
    );
    interaction.assert_consumed();

    shutdown_tx.send(true).unwrap();
    server_task.await.unwrap().unwrap();
}

#[tokio::test]
async fn cancellation_returns_before_creation_parameters_are_produced() {
    let args = create_args(&["coco", "create", "review/api", "-i"]);
    let mut interaction = ScriptedInteraction::new([0, 0, 0, 0, 0, 0, 0, 1], [] as [&str; 0]);

    let error = walkthrough(
        &test_paths(),
        PathBuf::from("/repo"),
        args,
        &mut interaction,
    )
    .await
    .unwrap_err();

    assert_eq!(error.to_string(), "workspace creation cancelled");
    interaction.assert_consumed();
}

#[test]
fn named_creation_stays_direct_unless_interactive_is_requested() {
    let direct = create_args(&["coco", "create", "review/api"]);
    assert!(!walkthrough_requested(&direct));

    let explicit = create_args(&["coco", "create", "review/api", "-i"]);
    assert!(walkthrough_requested(&explicit));

    let unnamed = create_args(&["coco", "create"]);
    assert!(walkthrough_requested(&unnamed));
}

fn create_args(arguments: &[&str]) -> CreateArgs {
    let cli = Cli::try_parse_from(arguments).unwrap();
    let Command::Create(args) = cli.command else {
        panic!("expected create command");
    };
    args
}

fn final_create_choice(interaction: &ScriptedInteraction) -> &Choice {
    let index = interaction
        .titles
        .iter()
        .rposition(|title| title == "Create workspace")
        .expect("the walkthrough did not reach its final confirmation");
    interaction.seen_choices[index]
        .first()
        .expect("the final confirmation did not contain its create choice")
}

fn choices_for_title<'a>(interaction: &'a ScriptedInteraction, title: &str) -> &'a [Choice] {
    let index = interaction
        .titles
        .iter()
        .position(|candidate| candidate == title)
        .unwrap_or_else(|| panic!("the walkthrough did not contain {title}"));
    &interaction.seen_choices[index]
}

fn assert_default_walkthrough_copy(interaction: &ScriptedInteraction) {
    let git_choices = choices_for_title(interaction, "Git changes");
    assert_eq!(
        git_choices
            .iter()
            .map(|choice| choice.label.as_str())
            .collect::<Vec<_>>(),
        ["Don't copy", "Copy tracked", "Copy tracked + untracked"]
    );
    assert_eq!(
        git_choices[1].detail.as_deref(),
        Some("fails if non-ignored untracked files exist")
    );
    assert_eq!(
        choices_for_title(interaction, "Profile")
            .iter()
            .map(|choice| choice.label.as_str())
            .collect::<Vec<_>>(),
        ["Use Codex config", "Choose profile"]
    );
    assert_eq!(
        choices_for_title(interaction, "After create")
            .iter()
            .map(|choice| choice.label.as_str())
            .collect::<Vec<_>>(),
        [
            "Return to shell",
            "Send message",
            "Open Codex",
            "Send, then open Codex",
        ]
    );
    for (title, choices) in interaction.titles.iter().zip(&interaction.seen_choices) {
        let default_count = choices.iter().filter(|choice| choice.is_default).count();
        if title == "Create workspace" {
            assert_eq!(default_count, 0);
        } else {
            assert!(
                choices[0].is_default,
                "{title} did not start on its default"
            );
            assert_eq!(default_count, 1, "{title} had an ambiguous default");
        }
        assert!(
            choices.iter().all(|choice| {
                choice
                    .detail
                    .as_deref()
                    .is_none_or(|detail| !detail.contains("default"))
            }),
            "{title} embedded the default marker in display text"
        );
    }
    assert_eq!(final_review_value(interaction, "Git changes"), "don't copy");
    assert_eq!(final_review_value(interaction, "Profile"), "Codex config");
    assert_eq!(final_review_value(interaction, "Model"), "configured model");
    assert_eq!(
        final_review_value(interaction, "After create"),
        "return to shell"
    );
}

fn final_review_fields(interaction: &ScriptedInteraction) -> &[ReviewField] {
    let index = interaction
        .titles
        .iter()
        .rposition(|title| title == "Create workspace")
        .expect("the walkthrough did not reach its final confirmation");
    &interaction.seen_review_fields[index]
}

fn final_review_value<'a>(interaction: &'a ScriptedInteraction, label: &str) -> &'a str {
    final_review_fields(interaction)
        .iter()
        .find(|field| field.label == label)
        .map(|field| field.value.as_str())
        .unwrap_or_else(|| panic!("the final review did not contain {label}"))
}

fn test_paths() -> CocoPaths {
    test_paths_at(PathBuf::from("/unreachable/coco-create-tests"))
}

fn test_paths_at(root: PathBuf) -> CocoPaths {
    CocoPaths {
        data_dir: root.clone(),
        database_path: root.join("coco.db"),
        socket_path: root.join("cocod.sock"),
        codex_endpoint_path: root.join("codex-app-server.json"),
        codex_token_path: root.join("codex-app-server.token"),
        worktrees_dir: root.join("worktrees"),
        codex_home: root.join("codex"),
        hooks_path: root.join("hooks.json"),
    }
}

struct WorkspaceListHandler {
    seen_params: Arc<Mutex<Vec<WorkspaceListParams>>>,
    items: Vec<WorkspaceListItem>,
}

struct UnregisteredRepositoryHandler;

#[async_trait]
impl RpcHandler for UnregisteredRepositoryHandler {
    async fn handle(&self, method: &str, _params: Value) -> Result<Value, RpcErrorPayload> {
        assert_eq!(method, "workspace.list");
        Err(RpcErrorPayload::new(
            "REPOSITORY_NOT_REGISTERED",
            "repository is not registered",
        ))
    }
}

#[async_trait]
impl RpcHandler for WorkspaceListHandler {
    async fn handle(&self, method: &str, params: Value) -> Result<Value, RpcErrorPayload> {
        if method != "workspace.list" {
            return Err(RpcErrorPayload::new("METHOD_NOT_FOUND", method));
        }
        let params = serde_json::from_value::<WorkspaceListParams>(params)
            .map_err(|error| RpcErrorPayload::new("INVALID_PARAMS", error.to_string()))?;
        self.seen_params.lock().unwrap().push(params.clone());
        let items = self
            .items
            .iter()
            .filter(|item| {
                params.phases.as_ref().is_none_or(|phases| {
                    phases
                        .iter()
                        .any(|phase| phase == item.workspace.phase.as_str())
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        serde_json::to_value(items)
            .map_err(|error| RpcErrorPayload::new("INTERNAL", error.to_string()))
    }
}

fn workspace_item() -> WorkspaceListItem {
    workspace_item_at(
        "workspace-source",
        "source/review",
        Path::new("/worktrees/source-review"),
    )
}

fn workspace_item_at(id: &str, name: &str, worktree_path: &Path) -> WorkspaceListItem {
    WorkspaceListItem {
        clients: None,
        workspace: Workspace {
            id: id.to_owned(),
            create_operation_id: None,
            repository_id: "repository-1".to_owned(),
            name: name.to_owned(),
            context_mode: ContextMode::Fresh,
            context: json!({}),
            profile: ProfileSnapshot {
                name: "default".to_owned(),
                source_path: None,
                source_hash: "profile-hash".to_owned(),
                model_override: None,
                effective_settings: json!({}),
            },
            lifecycle: WorkspaceLifecycle::Ready,
            availability: WorkspaceAvailability::Open,
            thread_runtime: None,
            phase: WorkspacePhase::Idle,
            wait_reasons: Vec::new(),
            worktree_mode: WorktreeMode::NewBranch,
            branch_name: Some(format!("coco/{name}")),
            base_sha: Some("base".to_owned()),
            worktree_path: Some(worktree_path.to_path_buf()),
            codex_thread_id: Some("thread-source".to_owned()),
            parent_thread_id: None,
            active_turn_id: None,
            last_error_code: None,
            last_error_message: None,
            created_at_ms: 1,
            updated_at_ms: 2,
            completed_at_ms: None,
            thread_archived: false,
            closed_head_sha: None,
            closed_at_ms: None,
        },
        repository: RepositorySummary {
            id: "repository-1".to_owned(),
            display_name: "repo".to_owned(),
            root_path: PathBuf::from("/repo"),
        },
        runtime_resources: None,
        activity: None,
    }
}

#[derive(Default)]
struct ScriptedInteraction {
    choices: VecDeque<usize>,
    texts: VecDeque<String>,
    titles: Vec<String>,
    seen_choices: Vec<Vec<Choice>>,
    seen_review_fields: Vec<Vec<ReviewField>>,
    notices: Vec<String>,
}

impl ScriptedInteraction {
    fn new(
        choices: impl IntoIterator<Item = usize>,
        texts: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            choices: choices.into_iter().collect(),
            texts: texts.into_iter().map(Into::into).collect(),
            titles: Vec::new(),
            seen_choices: Vec::new(),
            seen_review_fields: Vec::new(),
            notices: Vec::new(),
        }
    }

    fn assert_consumed(&self) {
        assert!(self.choices.is_empty(), "unused scripted choices");
        assert!(self.texts.is_empty(), "unused scripted text inputs");
    }
}

impl Interaction for ScriptedInteraction {
    fn is_interactive(&self) -> bool {
        true
    }

    fn select(&mut self, title: &str, choices: &[Choice]) -> Result<usize> {
        self.titles.push(title.to_owned());
        self.seen_choices.push(choices.to_vec());
        self.seen_review_fields.push(Vec::new());
        self.choices
            .pop_front()
            .context("test did not provide a selection")
    }

    fn select_with_review(
        &mut self,
        title: &str,
        fields: &[ReviewField],
        choices: &[Choice],
    ) -> Result<usize> {
        self.titles.push(title.to_owned());
        self.seen_choices.push(choices.to_vec());
        self.seen_review_fields.push(fields.to_vec());
        self.choices
            .pop_front()
            .context("test did not provide a selection")
    }

    fn confirm(&mut self, _title: &str) -> Result<bool> {
        panic!("the create walkthrough uses a picker for its final confirmation")
    }

    fn text(&mut self, _label: &str) -> Result<String> {
        self.texts.pop_front().context("test did not provide text")
    }

    fn notice(&mut self, message: &str) -> Result<()> {
        self.notices.push(message.to_owned());
        Ok(())
    }
}

mod choices;
mod context;
