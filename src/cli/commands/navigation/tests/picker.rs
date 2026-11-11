use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::watch;

use super::*;
use crate::cli::prompt::ReviewField;
use crate::rpc::{RpcErrorPayload, RpcHandler, RpcServer};

mod recency;

struct Capture {
    calls: Mutex<Vec<(String, Value)>>,
    unavailable: bool,
    empty: bool,
    outside_repository: bool,
    legacy_recency: bool,
}

#[async_trait]
impl RpcHandler for Capture {
    async fn handle(
        &self,
        method: &str,
        params: Value,
    ) -> std::result::Result<Value, RpcErrorPayload> {
        self.calls
            .lock()
            .unwrap()
            .push((method.to_owned(), params.clone()));
        match method {
            "repository.resolve" if self.outside_repository => Err(RpcErrorPayload::new(
                "NOT_A_GIT_REPOSITORY",
                "outside a registered repository",
            )),
            "repository.resolve" => {
                Ok(json!({"id":"repo", "displayName":"repo", "rootPath":"/repos/repo"}))
            }
            "repository.list" => Ok(json!([
                {"id":"repo", "displayName":"repo", "rootPath":"/repos/repo"}
            ])),
            "workspace.path.list" if self.empty => Ok(json!([])),
            "workspace.path.list" if self.legacy_recency && params["includeRecency"] == true => {
                Err(RpcErrorPayload::new(
                    "INVALID_PARAMS",
                    "unknown field includeRecency",
                ))
            }
            "workspace.path.list" => {
                let mut recent = entry("feat/10", "repo");
                if params["includeRecency"] == true {
                    recent.recency_at_ms = Some(NOW);
                }
                Ok(serde_json::to_value(vec![recent, entry("feat/2", "repo")]).unwrap())
            }
            "workspace.path.get" if self.unavailable => Err(RpcErrorPayload::new(
                "WORKTREE_UNAVAILABLE",
                "closed during selection",
            )),
            "workspace.path.get" => Ok(json!("/worktrees/repo/feat/2")),
            _ => panic!("navigation must not invoke {method}"),
        }
    }
}

#[derive(Default)]
struct Select {
    cancel: bool,
    choices: Vec<Choice>,
    noninteractive: bool,
    outside_repository: bool,
    repository_choices: Vec<Choice>,
}

impl Interaction for Select {
    fn is_interactive(&self) -> bool {
        !self.noninteractive
    }
    fn select(&mut self, title: &str, choices: &[Choice]) -> Result<usize> {
        if title == "Repository" {
            assert!(self.outside_repository);
            self.repository_choices = choices.to_vec();
        } else {
            assert_eq!(title, "Workspace directory");
            self.choices = choices.to_vec();
        }
        if self.cancel {
            bail!("selection cancelled");
        }
        Ok(0)
    }
    fn select_with_review(&mut self, _: &str, _: &[ReviewField], _: &[Choice]) -> Result<usize> {
        panic!("unexpected review")
    }
    fn confirm(&mut self, _: &str) -> Result<bool> {
        panic!("unexpected confirmation")
    }
    fn text(&mut self, _: &str) -> Result<String> {
        panic!("unexpected text input")
    }
    fn notice(&mut self, _: &str) -> Result<()> {
        panic!("unexpected notice")
    }
}

async fn exercise(
    global: bool,
    unavailable: bool,
    empty: bool,
    interaction: &mut Select,
) -> (Result<PathBuf>, Vec<(String, Value)>) {
    exercise_selection(
        Options {
            global,
            unavailable,
            empty,
            sort: WorkspaceSort::Name,
            ..Default::default()
        },
        interaction,
    )
    .await
}

#[derive(Default)]
struct Options {
    global: bool,
    unavailable: bool,
    empty: bool,
    sort: WorkspaceSort,
    legacy_recency: bool,
    workspace: Option<String>,
}

async fn exercise_selection(
    options: Options,
    interaction: &mut Select,
) -> (Result<PathBuf>, Vec<(String, Value)>) {
    let root = tempfile::tempdir().unwrap();
    let paths = CocoPaths {
        data_dir: root.path().to_owned(),
        database_path: root.path().join("state.db"),
        socket_path: root.path().join("rpc.sock"),
        codex_endpoint_path: root.path().join("endpoint"),
        codex_token_path: root.path().join("token"),
        worktrees_dir: root.path().join("worktrees"),
        codex_home: root.path().join("codex"),
        hooks_path: root.path().join("hooks"),
    };
    let capture = Arc::new(Capture {
        calls: Mutex::new(Vec::new()),
        unavailable: options.unavailable,
        empty: options.empty,
        outside_repository: interaction.outside_repository,
        legacy_recency: options.legacy_recency,
    });
    let server = RpcServer::bind(&paths.socket_path, capture.clone())
        .await
        .unwrap();
    let (shutdown, receiver) = watch::channel(false);
    let task = tokio::spawn(server.run(receiver));
    let result = resolve_path(
        &paths,
        WorkspaceSelection {
            repository_scope: RepositoryScope::repository(if interaction.outside_repository {
                "/outside"
            } else {
                "/repos/repo"
            }),
            has_explicit_path: false,
            workspace: options.workspace,
            global: options.global,
            sort: options.sort,
        },
        interaction,
    )
    .await;
    shutdown.send(true).unwrap();
    task.await.unwrap().unwrap();
    let calls = capture.calls.lock().unwrap().clone();
    (result, calls)
}

#[tokio::test]
async fn outside_repository_picker_precedes_workspace_selection_and_can_cancel_safely() {
    for cancel in [false, true] {
        let mut interaction = Select {
            outside_repository: true,
            cancel,
            ..Select::default()
        };
        let (result, calls) = exercise(false, false, false, &mut interaction).await;
        assert_eq!(
            interaction.repository_choices,
            [Choice::new("repo", Some("/repos/repo".to_owned()))]
        );
        assert_eq!(calls[0].0, "repository.resolve");
        assert_eq!(calls[0].1["path"], "/outside");
        assert_eq!(calls[1].0, "repository.list");
        if cancel {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("selection cancelled")
            );
            assert_eq!(calls.len(), 2);
            assert!(interaction.choices.is_empty());
        } else {
            assert_eq!(result.unwrap(), PathBuf::from("/worktrees/repo/feat/2"));
            assert_eq!(calls.len(), 4);
            assert_eq!(calls[2].0, "workspace.path.list");
            assert_eq!(calls[3].0, "workspace.path.get");
            assert_eq!(calls[3].1["workspace"], "repo-feat/2");
            for (_, params) in &calls[2..] {
                assert_eq!(
                    params["scope"],
                    json!({"kind":"repository", "path":"/repos/repo"})
                );
            }
        }
    }
}

#[tokio::test]
async fn picker_sorts_saved_metadata_and_revalidates_the_selected_uuid_in_the_same_scope() {
    for global in [false, true] {
        let mut interaction = Select::default();
        let (path, calls) = exercise(global, false, false, &mut interaction).await;
        assert_eq!(path.unwrap(), PathBuf::from("/worktrees/repo/feat/2"));
        assert_eq!(interaction.choices[0].label, "feat/2");
        assert_eq!(interaction.choices[1].label, "feat/10");
        let get = calls.last().unwrap();
        assert_eq!(get.0, "workspace.path.get");
        assert_eq!(get.1["workspace"], "repo-feat/2");
        assert_eq!(
            get.1["scope"]["kind"],
            if global {
                "allRepositories"
            } else {
                "repository"
            }
        );
        assert!(
            calls
                .iter()
                .all(|(method, _)| method == "repository.resolve"
                    || method.starts_with("workspace.path."))
        );
    }
}

#[tokio::test]
async fn cancelled_empty_and_noninteractive_pickers_do_not_resolve_a_destination() {
    for (cancel, empty, noninteractive) in [
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        let mut interaction = Select {
            cancel,
            noninteractive,
            ..Select::default()
        };
        let (result, calls) = exercise(true, false, empty, &mut interaction).await;
        assert!(result.is_err());
        assert!(
            !calls
                .iter()
                .any(|(method, _)| method == "workspace.path.get")
        );
        if noninteractive {
            assert!(calls.is_empty());
        }
    }
}

#[tokio::test]
async fn a_worktree_closed_during_selection_fails_instead_of_using_the_stale_path() {
    let (result, calls) = exercise(true, true, false, &mut Select::default()).await;
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("WORKTREE_UNAVAILABLE")
    );
    assert_eq!(calls.last().unwrap().1["workspace"], "repo-feat/2");
}
