use super::*;

mod native;

fn record(upstream: &str, remote: &str, track: &str) -> String {
    format!(
        "refs/heads/main\0{}\0\0{upstream}\0{remote}\0{track}\0\n",
        "a".repeat(40)
    )
}

#[test]
fn counts_come_from_native_tracking_not_branch_spelling() {
    use GitTrackingState::*;
    for (track, state, ahead, behind) in [
        ("", InSync, 0, 0),
        ("ahead 2", Ahead, 2, 0),
        ("behind 3", Behind, 0, 3),
        ("ahead 2, behind 3", Diverged, 2, 3),
    ] {
        let branches =
            parse::branches(&record("refs/remotes/upstream/release", "upstream", track)).unwrap();
        let tracking = &branches["refs/heads/main"].tracking;
        assert_eq!(tracking.state, state);
        assert_eq!(tracking.ahead_by, Some(ahead));
        assert_eq!(tracking.behind_by, Some(behind));
        assert_eq!(
            tracking.upstream.as_deref(),
            Some("refs/remotes/upstream/release")
        );
        assert_eq!(tracking.remote.as_deref(), Some("upstream"));
    }
}

#[test]
fn absent_and_gone_upstreams_do_not_turn_into_zero_counts() {
    for (upstream, remote, track, state) in [
        ("", "", "", GitTrackingState::NoUpstream),
        (
            "refs/remotes/origin/main",
            "origin",
            "gone",
            GitTrackingState::UpstreamMissing,
        ),
    ] {
        let branches = parse::branches(&record(upstream, remote, track)).unwrap();
        let tracking = &branches["refs/heads/main"].tracking;
        assert_eq!(tracking.state, state);
        assert!(tracking.ahead_by.is_none() && tracking.behind_by.is_none());
    }
}

#[test]
fn malformed_or_duplicate_native_metadata_is_unknown_not_success() {
    for track in [
        "ahead -1",
        "ahead 18446744073709551616",
        "ahead 1, ahead 2",
        "behind ?",
        "nonsense",
        "ahead 1,",
    ] {
        assert!(parse::branches(&record("refs/remotes/origin/main", "origin", track)).is_err());
    }
    assert!(parse::branches(&record("", "", "ahead 1")).is_err());
    assert!(parse::branches(&record("refs/heads/main", ".", "").repeat(2)).is_err());
    assert!(parse::branches("refs/heads/main\0truncated").is_err());
}

#[test]
fn worktree_bindings_and_sha_must_match_the_same_inventory() {
    let commit = "a".repeat(40);
    let snapshot = TrackingSnapshot {
        branches: parse::branches(&record("refs/heads/trunk", ".", "ahead 1")).unwrap(),
        worktrees: parse::worktrees(&format!(
            "worktree /tmp/work\0HEAD {commit}\0branch refs/heads/main\0\0"
        ))
        .unwrap(),
        observed_at_ms: 42,
    };
    let tracking = snapshot.worktree(
        Path::new("/tmp/work"),
        WorktreeMode::NewBranch,
        Some("main"),
        Some(&commit),
    );
    assert_eq!(tracking.state, GitTrackingState::Ahead);
    assert_eq!(tracking.observed_at_ms, Some(42));
    for (path, mode, branch, head) in [
        ("/tmp/missing", WorktreeMode::NewBranch, Some("main"), None),
        ("/tmp/work", WorktreeMode::NewBranch, Some("changed"), None),
        ("/tmp/work", WorktreeMode::Detached, None, None),
        (
            "/tmp/work",
            WorktreeMode::NewBranch,
            Some("main"),
            Some("wrong"),
        ),
    ] {
        assert_eq!(
            snapshot.worktree(Path::new(path), mode, branch, head).state,
            GitTrackingState::Unknown
        );
    }
    assert_eq!(
        snapshot.branch("refs/heads/main", &"b".repeat(40)).state,
        GitTrackingState::Unknown
    );
}

#[test]
fn detached_unborn_bare_and_malformed_records_stay_distinct() {
    let commit = "a".repeat(40);
    let text = format!(
        "worktree /tmp/detached\0HEAD {commit}\0detached\0\0worktree /tmp/unborn\0HEAD {}\0branch refs/heads/main\0\0worktree /tmp/bare\0bare\0\0",
        "0".repeat(40)
    );
    let worktrees = parse::worktrees(&text).unwrap();
    assert_eq!(worktrees.len(), 1);
    assert!(worktrees[Path::new("/tmp/detached")].branch.is_none());
    assert!(
        parse::worktrees(&format!(
            "worktree /tmp/x\0HEAD {commit}\0detached\0branch refs/heads/main\0\0"
        ))
        .is_err()
    );
    assert!(parse::worktrees("worktree relative\0bare\0\0").is_err());
}

#[test]
fn skipped_main_worktree_does_not_promote_a_linked_record() {
    let worktrees = parse::worktrees(&format!(
        "worktree /tmp/bare\0bare\0\0worktree /tmp/child\0HEAD {}\0branch refs/heads/main\0\0",
        "a".repeat(40)
    ))
    .unwrap();
    assert!(!worktrees[Path::new("/tmp/child")].main);
}
