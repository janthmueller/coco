use super::*;

struct ContextHandler {
    repository: Option<RepositorySummary>,
    items: Vec<WorkspaceListItem>,
    calls: Arc<Mutex<Vec<(String, Value)>>>,
}

#[async_trait]
impl RpcHandler for ContextHandler {
    async fn handle(&self, method: &str, params: Value) -> Result<Value, RpcErrorPayload> {
        self.calls
            .lock()
            .unwrap()
            .push((method.to_owned(), params.clone()));
        match method {
            "repository.resolve" => self
                .repository
                .as_ref()
                .map(|repository| serde_json::to_value(repository).unwrap())
                .ok_or_else(|| RpcErrorPayload::new("REPOSITORY_NOT_REGISTERED", "not registered")),
            "workspace.list" => {
                let params: WorkspaceListParams = serde_json::from_value(params).unwrap();
                Ok(serde_json::to_value(
                    self.items
                        .iter()
                        .filter(|item| {
                            (matches!(params.scope, RepositoryScope::AllRepositories)
                                || self
                                    .repository
                                    .as_ref()
                                    .is_some_and(|repository| repository.id == item.repository.id))
                                && params.phases.as_ref().is_none_or(|phases| {
                                    phases
                                        .iter()
                                        .any(|phase| phase == item.workspace.phase.as_str())
                                })
                        })
                        .collect::<Vec<_>>(),
                )
                .unwrap())
            }
            _ => panic!("unexpected discovery method {method}"),
        }
    }
}

fn item(repository: &str, id: &str, name: &str) -> WorkspaceListItem {
    let mut item = workspace_item_at(id, name, Path::new("/unused-worktree"));
    item.workspace.repository_id = repository.to_owned();
    item.repository.id = repository.to_owned();
    item.repository.root_path = PathBuf::from(format!("/projects/{repository}"));
    item
}

async fn run_guide(
    paths: &CocoPaths,
    scope: PathBuf,
    repository: Option<RepositorySummary>,
    items: Vec<WorkspaceListItem>,
    selections: &[usize],
    sort: WorkspaceSort,
) -> (CreateArgs, ScriptedInteraction, Vec<(String, Value)>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let handler = Arc::new(ContextHandler {
        repository,
        items,
        calls: Arc::clone(&calls),
    });
    let server = RpcServer::bind(&paths.socket_path, handler).await.unwrap();
    let (shutdown, receiver) = watch::channel(false);
    let server_task = tokio::spawn(server.run(receiver));
    let mut interaction = ScriptedInteraction::new(selections.iter().copied(), [] as [&str; 0]);
    let args = walkthrough(
        paths,
        scope,
        create_args(&[
            "coco",
            "create",
            "review/new",
            "-i",
            "--sort",
            match sort {
                WorkspaceSort::Recent => "recent",
                WorkspaceSort::Name => "name",
            },
        ]),
        &mut interaction,
    )
    .await
    .unwrap();
    interaction.assert_consumed();
    shutdown.send(true).unwrap();
    server_task.await.unwrap().unwrap();
    let recorded = calls.lock().unwrap().clone();
    (args, interaction, recorded)
}

#[tokio::test]
async fn context_uses_global_natural_ties_and_labels_duplicate_names() {
    let temporary = tempdir().unwrap();
    let paths = test_paths_at(temporary.path().to_path_buf());
    let local = item("z-current", "local-2", "review/w2");
    let mut prepared = item("a-other", "prepared", "review/prepared");
    prepared.workspace.phase = WorkspacePhase::Prepared;
    let (args, interaction, calls) = run_guide(
        &paths,
        PathBuf::from("/projects/z-current/nested"),
        Some(local.repository.clone()),
        vec![
            item("z-other", "last", "review/a"),
            item("z-current", "local-10", "review/w10"),
            item("a-other", "foreign", "review/w2"),
            prepared,
            local,
        ],
        &[0, 0, 0, 1, 1, 0, 0, 0, 0, 0],
        WorkspaceSort::Recent,
    )
    .await;
    assert_eq!(args.context.as_deref(), Some("workspace:foreign"));
    let choices = choices_for_title(&interaction, "Context from workspace");
    assert_eq!(
        choices
            .iter()
            .map(|choice| choice.label.as_str())
            .collect::<Vec<_>>(),
        ["review/a", "review/w2", "review/w2", "review/w10"]
    );
    assert!(
        choices[2]
            .detail
            .as_deref()
            .unwrap()
            .contains("/projects/z-current")
    );
    assert!(
        choices[1]
            .detail
            .as_deref()
            .unwrap()
            .contains("/projects/a-other")
    );
    assert_eq!(
        final_review_value(&interaction, "Context"),
        "workspace review/w2 · /projects/a-other"
    );
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "workspace.list");
    assert_eq!(calls[0].1["scope"]["kind"], "allRepositories");
    let (params, _, _) = normalize_create_args(
        PathBuf::from("/projects/z-current/nested"),
        args,
        "create-cross-repo".to_owned(),
    )
    .unwrap();
    assert_eq!(
        params.context,
        WorkspaceContextRequest::Fork {
            source: WorkspaceContextSource::Reference {
                reference: "workspace:foreign".to_owned()
            },
            compact: false
        }
    );
}

#[tokio::test]
async fn an_unregistered_target_can_select_registered_context_without_changing_its_code_base() {
    let temporary = tempdir().unwrap();
    let paths = test_paths_at(temporary.path().to_path_buf());
    let (args, interaction, _) = run_guide(
        &paths,
        PathBuf::from("/new/repo"),
        None,
        vec![item("other", "foreign", "investigate/api")],
        &[0, 0, 0, 1, 0, 0, 0, 0, 0, 0],
        WorkspaceSort::Recent,
    )
    .await;
    assert_eq!(args.context.as_deref(), Some("workspace:foreign"));
    assert!(args.base_workspace.is_none());
    assert!(args.base.is_none());
    assert!(choices_for_title(&interaction, "Context")[0].is_default);
}

#[tokio::test]
async fn code_picker_stays_local_while_context_picker_crosses_repositories() {
    let temporary = tempdir().unwrap();
    let paths = test_paths_at(temporary.path().to_path_buf());
    let local = item("current", "local", "investigate/api");
    let (args, interaction, calls) = run_guide(
        &paths,
        PathBuf::from("/projects/current"),
        Some(local.repository.clone()),
        vec![item("another", "foreign", "investigate/api"), local],
        &[0, 2, 0, 1, 0, 0, 0, 0, 0, 0],
        WorkspaceSort::Recent,
    )
    .await;
    assert_eq!(args.base_workspace.as_deref(), Some("local"));
    assert_eq!(args.context.as_deref(), Some("workspace:foreign"));
    assert_eq!(
        choices_for_title(&interaction, "Code from workspace").len(),
        1
    );
    assert_eq!(
        choices_for_title(&interaction, "Context from workspace").len(),
        2
    );
    assert_eq!(calls[0].1["scope"]["kind"], "repository");
    assert_eq!(calls.last().unwrap().1["scope"]["kind"], "allRepositories");
}

#[tokio::test]
async fn another_context_from_a_nested_worktree_uses_global_order_and_omits_the_current_owner() {
    let temporary = tempdir().unwrap();
    let paths = test_paths_at(temporary.path().to_path_buf());
    let worktree = paths.worktrees_dir.join("current/workspace");
    let nested = worktree.join("src");
    std::fs::create_dir_all(&nested).unwrap();
    let mut current = item("z-current", "current", "current");
    current.workspace.worktree_path = Some(worktree);
    let (args, interaction, calls) = run_guide(
        &paths,
        nested.clone(),
        Some(current.repository.clone()),
        vec![
            item("a-other", "foreign", "review/a"),
            current,
            item("z-current", "local", "review/z"),
        ],
        &[0, 0, 0, 2, 0, 0, 0, 0, 0, 0],
        WorkspaceSort::Recent,
    )
    .await;
    assert_eq!(args.context.as_deref(), Some("workspace:foreign"));
    let choices = choices_for_title(&interaction, "Context from workspace");
    assert_eq!(
        choices
            .iter()
            .map(|choice| choice.label.as_str())
            .collect::<Vec<_>>(),
        ["review/a", "review/z"]
    );
    assert_eq!(calls[0].1["scope"]["path"], json!(nested));
    assert_eq!(calls[1].1["scope"]["kind"], "allRepositories");
}

fn recent_item(repository: &str, id: &str, name: &str, at: i64) -> WorkspaceListItem {
    let mut item = item(repository, id, name);
    item.workspace.thread_runtime = Some(
        serde_json::from_value(json!({
            "status": {"type": "idle"}, "recencyAtMs": at,
            "runtimeGeneration": "test", "observedAtMs": at, "isFresh": true
        }))
        .unwrap(),
    );
    item
}

#[tokio::test]
async fn create_sorts_local_code_and_cross_repository_context_by_recent_use_or_name() {
    for (sort, code_id, context_id) in [
        (WorkspaceSort::Recent, "local-new", "foreign"),
        (WorkspaceSort::Name, "local-old", "local-old"),
    ] {
        let temporary = tempdir().unwrap();
        let paths = test_paths_at(temporary.path().to_path_buf());
        let old = recent_item("current", "local-old", "work/a", 1_800_000_000_000);
        let (args, interaction, calls) = run_guide(
            &paths,
            PathBuf::from("/projects/current"),
            Some(old.repository.clone()),
            vec![
                recent_item("other", "foreign", "work/a", 1_800_000_200_000),
                old,
                recent_item("current", "local-new", "work/z", 1_800_000_100_000),
            ],
            &[0, 2, 0, 1, 0, 0, 0, 0, 0, 0],
            sort,
        )
        .await;
        assert_eq!(args.base_workspace.as_deref(), Some(code_id));
        assert_eq!(
            args.context.as_deref(),
            Some(format!("workspace:{context_id}").as_str())
        );
        let code = choices_for_title(&interaction, "Code from workspace");
        let context = choices_for_title(&interaction, "Context from workspace");
        assert_eq!(code.len(), 2);
        assert_eq!(context.len(), 3);
        assert!(code[0].trailing_detail.is_some());
        assert!(context[0].trailing_detail.is_some());
        assert!(
            context[0]
                .detail
                .as_deref()
                .unwrap()
                .contains(if sort == WorkspaceSort::Recent {
                    "/projects/other"
                } else {
                    "/projects/current"
                })
        );
        assert_eq!(calls[0].1["scope"]["kind"], "repository");
        assert_eq!(calls[1].1["scope"]["kind"], "allRepositories");
        let request: WorkspaceListParams = serde_json::from_value(calls[1].1.clone()).unwrap();
        assert!(!request.include_resources);
        assert!(!request.include_activity);
    }
}
