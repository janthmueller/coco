use super::*;
use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};
use async_trait::async_trait;
use tokio::sync::watch;

use crate::cli::commands::{
    ResolvedWorkspaceTarget, WorkspaceSelection, resolve_workspace_input_with_picker,
};
use crate::cli::prompt::{Interaction, ReviewField};
use crate::paths::CocoPaths;
use crate::protocol::RepositoryScope;
use crate::rpc::{RpcErrorPayload, RpcHandler, RpcServer};

struct Listed {
    rows: Vec<WorkspaceListItem>,
    calls: Mutex<Vec<serde_json::Value>>,
}

#[async_trait]
impl RpcHandler for Listed {
    async fn handle(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, RpcErrorPayload> {
        if method == "repository.resolve" {
            return Ok(json!({"id":"local", "displayName":"local", "rootPath":params["path"]}));
        }
        assert_eq!(method, "workspace.list");
        self.calls.lock().unwrap().push(params);
        Ok(serde_json::to_value(&self.rows).unwrap())
    }
}

struct Pick {
    selected: usize,
    choices: Vec<Choice>,
    cancel: bool,
    interactive: bool,
}

impl Interaction for Pick {
    fn is_interactive(&self) -> bool {
        self.interactive
    }
    fn select(&mut self, _: &str, choices: &[Choice]) -> Result<usize> {
        self.choices = choices.to_vec();
        if self.cancel {
            bail!("selection cancelled");
        }
        Ok(self.selected)
    }
    fn select_with_review(&mut self, _: &str, _: &[ReviewField], _: &[Choice]) -> Result<usize> {
        panic!("unexpected review");
    }
    fn confirm(&mut self, _: &str) -> Result<bool> {
        panic!("unexpected confirmation");
    }
    fn text(&mut self, _: &str) -> Result<String> {
        panic!("unexpected input");
    }
    fn notice(&mut self, _: &str) -> Result<()> {
        panic!("unexpected notice");
    }
}

fn paths(root: &std::path::Path) -> CocoPaths {
    CocoPaths {
        data_dir: root.to_owned(),
        database_path: root.join("state.db"),
        socket_path: root.join("rpc.sock"),
        codex_home: root.join("codex"),
        codex_endpoint_path: root.join("endpoint.json"),
        codex_token_path: root.join("token"),
        worktrees_dir: root.join("worktrees"),
        hooks_path: root.join("hooks.json"),
    }
}

#[tokio::test]
async fn sorted_picker_uses_the_selected_exact_id_and_only_one_passive_scoped_query() {
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    let handler = Arc::new(Listed {
        rows: vec![
            row("alpha", "old", Some(1)),
            row("zeta", "recent", Some(NOW)),
        ],
        calls: Mutex::new(Vec::new()),
    });
    let server = RpcServer::bind(&paths.socket_path, handler.clone())
        .await
        .unwrap();
    let (stop, shutdown) = watch::channel(false);
    let server = tokio::spawn(server.run(shutdown));
    let mut picker = Pick {
        selected: 0,
        choices: vec![],
        cancel: false,
        interactive: true,
    };
    let selection = || WorkspaceSelection {
        repository_scope: RepositoryScope::AllRepositories,
        has_explicit_path: false,
        workspace: None,
        global: true,
    };
    let chosen = resolve_workspace_input_with_picker(
        &paths,
        selection(),
        "Open workspace",
        None,
        Some(JumpSort::Recent),
        &mut picker,
    )
    .await
    .unwrap();
    assert!(
        matches!(chosen, ResolvedWorkspaceTarget { workspace, verified: true, .. } if workspace == "zeta-recent")
    );
    assert_eq!(names(&picker.choices), ["recent", "old"]);
    assert_eq!(handler.calls.lock().unwrap().len(), 1);
    assert_eq!(
        handler.calls.lock().unwrap()[0]["scope"]["kind"],
        "allRepositories"
    );
    picker.cancel = true;
    assert!(
        resolve_workspace_input_with_picker(
            &paths,
            selection(),
            "Open workspace",
            None,
            Some(JumpSort::Recent),
            &mut picker
        )
        .await
        .is_err()
    );
    assert_eq!(handler.calls.lock().unwrap().len(), 2);
    stop.send(true).unwrap();
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn explicit_targets_and_no_input_do_not_query_or_open_a_picker() {
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    let mut picker = Pick {
        selected: 0,
        choices: vec![],
        cancel: false,
        interactive: false,
    };
    let selection = |workspace| WorkspaceSelection {
        repository_scope: RepositoryScope::repository(temp.path()),
        has_explicit_path: true,
        workspace,
        global: false,
    };
    let target = resolve_workspace_input_with_picker(
        &paths,
        selection(Some("exact-name".into())),
        "Open",
        None,
        Some(JumpSort::Recent),
        &mut picker,
    )
    .await
    .unwrap();
    assert_eq!(target.workspace, "exact-name");
    assert_eq!(target.scope, RepositoryScope::repository(temp.path()));
    assert!(!target.verified);
    let error = resolve_workspace_input_with_picker(
        &paths,
        selection(None),
        "Open",
        None,
        Some(JumpSort::Recent),
        &mut picker,
    )
    .await
    .err()
    .unwrap();
    assert!(error.to_string().contains("workspace is required"));
    assert!(picker.choices.is_empty());
}

#[tokio::test]
async fn local_scope_name_override_and_empty_results_keep_existing_selection_safety() {
    for empty in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let paths = paths(temp.path());
        let handler = Arc::new(Listed {
            rows: if empty {
                vec![]
            } else {
                vec![
                    row("local", "z-recent", Some(NOW)),
                    row("local", "a-old", Some(1)),
                ]
            },
            calls: Mutex::new(Vec::new()),
        });
        let server = RpcServer::bind(&paths.socket_path, handler.clone())
            .await
            .unwrap();
        let (stop, shutdown) = watch::channel(false);
        let server = tokio::spawn(server.run(shutdown));
        let mut picker = Pick {
            selected: 0,
            choices: vec![],
            cancel: false,
            interactive: true,
        };
        let result = resolve_workspace_input_with_picker(
            &paths,
            WorkspaceSelection {
                repository_scope: RepositoryScope::repository(temp.path()),
                has_explicit_path: true,
                workspace: None,
                global: false,
            },
            "Open",
            None,
            Some(JumpSort::Name),
            &mut picker,
        )
        .await;
        if empty {
            assert!(
                result
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("no workspaces are available")
            );
            assert!(picker.choices.is_empty());
        } else {
            assert_eq!(result.unwrap().workspace, "local-a-old");
            assert_eq!(names(&picker.choices), ["a-old", "z-recent"]);
            assert!(
                !picker.choices[0]
                    .detail
                    .as_ref()
                    .unwrap()
                    .contains("/repos/")
            );
        }
        let calls = handler.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0]["scope"],
            json!({"kind":"repository", "path":temp.path()})
        );
        let request: crate::protocol::WorkspaceListParams =
            serde_json::from_value(calls[0].clone()).unwrap();
        assert!(
            !request.include_resources && !request.include_activity && !request.include_clients
        );
        stop.send(true).unwrap();
        server.await.unwrap().unwrap();
    }
}
