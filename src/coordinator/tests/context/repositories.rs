use super::*;

async fn sources(fixture: &Fixture) -> (Workspace, Workspace) {
    fixture.register().await;
    let local = fixture
        .create_and_materialize(fresh_create_params(
            fixture.source.clone(),
            "review/shared",
            "local-source",
        ))
        .await;
    let other = fixture._temp.path().join("other-repository");
    initialize_repository(&other);
    fs::write(other.join("README.md"), "other repository\n").unwrap();
    run_git(&other, &["add", "README.md"]);
    run_git(&other, &["commit", "-m", "other code"]);
    let foreign = fixture
        .create_and_materialize(fresh_create_params(
            other,
            "review/shared",
            "foreign-source",
        ))
        .await;
    (local, foreign)
}

#[tokio::test]
async fn exact_context_ids_cross_repositories_and_materialize_without_copying_the_source_code() {
    let fixture = Fixture::new(FakeWorker::default());
    let (local, foreign) = sources(&fixture).await;
    for (index, source) in [
        WorkspaceContextSource::Workspace {
            workspace: foreign.id.clone(),
        },
        WorkspaceContextSource::Reference {
            reference: format!("workspace:{}", foreign.id),
        },
        WorkspaceContextSource::Reference {
            reference: foreign.id.clone(),
        },
    ]
    .into_iter()
    .enumerate()
    {
        let mut params = fresh_create_params(
            fixture.source.clone(),
            &format!("child-{index}"),
            &format!("create-child-{index}"),
        );
        params.context = WorkspaceContextRequest::Fork {
            source,
            compact: false,
        };
        let child = fixture
            .coordinator
            .create_workspace(params)
            .await
            .unwrap()
            .workspace;
        assert_eq!(child.repository_id, local.repository_id);
        assert_eq!(child.parent_thread_id, foreign.codex_thread_id);
        assert!(child.codex_thread_id.is_some());
        assert_eq!(child.phase, WorkspacePhase::Idle);
        assert_eq!(
            child.context["resolved"]["context"]["source"]["workspaceId"],
            foreign.id
        );
        let worktree = child.worktree_path.as_deref().unwrap();
        assert_eq!(
            fs::read_to_string(worktree.join("README.md")).unwrap(),
            "fixture\n"
        );
        assert_eq!(
            git_output(worktree, &["rev-parse", "HEAD"]),
            git_output(&fixture.source, &["rev-parse", "HEAD"])
        );
        assert!(
            matches!(fixture.worker.calls().last(), Some(WorkerCall::Fork { source_thread_id, last_turn_id, cwd, .. })
            if Some(source_thread_id) == foreign.codex_thread_id.as_ref() && cwd == worktree
                && last_turn_id.as_deref() == Some(&format!("{source_thread_id}-completed")))
        );
    }
}

#[tokio::test]
async fn names_remain_local_even_when_a_foreign_workspace_has_the_same_name() {
    let fixture = Fixture::new(FakeWorker::default());
    let (local, _) = sources(&fixture).await;
    let mut params = fixture.create_params();
    params.context = WorkspaceContextRequest::Fork {
        source: WorkspaceContextSource::Reference {
            reference: local.name.clone(),
        },
        compact: false,
    };
    let child = fixture
        .coordinator
        .create_workspace(params)
        .await
        .unwrap()
        .workspace;
    assert_eq!(child.parent_thread_id, local.codex_thread_id);
    assert_eq!(
        child.context["resolved"]["context"]["source"]["workspaceId"],
        local.id
    );
}

#[tokio::test]
async fn foreign_names_and_code_ids_do_not_gain_implicit_global_scope() {
    let fixture = Fixture::new(FakeWorker::default());
    let (_, foreign) = sources(&fixture).await;
    let repository = foreign.worktree_path.as_ref().unwrap().clone();
    let mut source_params = fresh_create_params(repository, "foreign-only", "create-foreign-only");
    source_params.context = WorkspaceContextRequest::Fresh;
    fixture.create_and_materialize(source_params).await;
    let before = fixture.worker.calls().len();
    let mut params = fixture.create_params();
    params.context = WorkspaceContextRequest::Fork {
        source: WorkspaceContextSource::Reference {
            reference: "foreign-only".to_owned(),
        },
        compact: false,
    };
    let error = fixture
        .coordinator
        .create_workspace(params)
        .await
        .unwrap_err();
    assert_eq!(error.code(), "WORKSPACE_NOT_FOUND");
    let mut params = fixture.create_params();
    params.worktree = WorkspaceWorktreeRequest::NewBranch {
        branch: None,
        base: WorkspaceBaseRequest::Workspace {
            workspace: foreign.id.clone(),
        },
    };
    let error = fixture
        .coordinator
        .create_workspace(params)
        .await
        .unwrap_err();
    assert_eq!(error.code(), "WORKSPACE_NOT_FOUND");
    assert_eq!(fixture.worker.calls().len(), before);
    assert!(
        fixture
            .coordinator
            .resolve_workspace(
                &RepositoryScope::repository(fixture.source.clone()),
                &foreign.id
            )
            .is_err()
    );
}
