use super::*;
use crate::protocol::{RepositoryListParams, RepositoryRemoveParams, RepositoryResolveParams};

#[tokio::test]
async fn workspace_creation_enrolls_an_unregistered_repository() {
    let fixture = Fixture::new(FakeWorker::default());
    assert!(
        fixture
            .coordinator
            .list_repositories(RepositoryListParams {})
            .unwrap()
            .is_empty()
    );

    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;
    let repositories = fixture
        .coordinator
        .list_repositories(RepositoryListParams {})
        .unwrap();

    assert_eq!(repositories.len(), 1);
    assert_eq!(workspace.repository_id, repositories[0].id);
    assert_eq!(repositories[0].root_path, fixture.source);
}

#[tokio::test]
async fn repository_reads_do_not_enroll_an_unregistered_repository() {
    let fixture = Fixture::new(FakeWorker::default());

    assert!(matches!(
        fixture
            .coordinator
            .resolve_repository(RepositoryResolveParams {
                path: fixture.source.clone(),
            }),
        Err(CoordinatorError::RepositoryNotRegistered(path)) if path == fixture.source
    ));
    assert!(
        fixture
            .coordinator
            .list_repositories(RepositoryListParams {})
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn empty_repository_can_be_removed_and_reenrolled_with_the_same_identity() {
    let fixture = Fixture::new(FakeWorker::default());
    let registered = fixture.register().await;

    let removed = fixture
        .coordinator
        .remove_repository(RepositoryRemoveParams {
            path: fixture.source.clone(),
        })
        .await
        .unwrap();
    assert_eq!(removed.id, registered.id);
    assert!(fixture.source.is_dir());
    assert!(
        fixture
            .coordinator
            .list_repositories(RepositoryListParams {})
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        fixture
            .coordinator
            .resolve_repository(RepositoryResolveParams {
                path: fixture.source.clone(),
            }),
        Err(CoordinatorError::RepositoryNotRegistered(_))
    ));

    let reactivated = fixture.register().await;
    assert_eq!(reactivated.id, registered.id);
}

#[tokio::test]
async fn a_missing_registered_checkout_can_still_be_removed_by_its_exact_root() {
    let fixture = Fixture::new(FakeWorker::default());
    let registered = fixture.register().await;
    fs::remove_dir_all(&fixture.source).unwrap();

    let removed = fixture
        .coordinator
        .remove_repository(RepositoryRemoveParams {
            path: fixture.source.clone(),
        })
        .await
        .unwrap();

    assert_eq!(removed.id, registered.id);
    assert!(
        fixture
            .coordinator
            .list_repositories(RepositoryListParams {})
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn repository_removal_is_blocked_while_any_workspace_record_remains() {
    let fixture = Fixture::new(FakeWorker::default());
    let workspace = fixture
        .coordinator
        .create_workspace(fixture.create_params())
        .await
        .unwrap()
        .workspace;

    let error = fixture
        .coordinator
        .remove_repository(RepositoryRemoveParams {
            path: fixture.source.clone(),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        CoordinatorError::RepositoryHasWorkspaces {
            path,
            workspace_count: 1,
        } if path == fixture.source
    ));
    assert!(workspace.worktree_path.unwrap().is_dir());
    assert_eq!(
        fixture
            .coordinator
            .list_repositories(RepositoryListParams {})
            .unwrap()
            .len(),
        1
    );
}
