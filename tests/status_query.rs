#![cfg(unix)]

use anyhow::Result;
use serde_json::json;

#[path = "status_query/follow.rs"]
mod follow;
#[path = "status_query/support.rs"]
mod support;
#[path = "status_query/views.rs"]
mod views;

use support::{Fixture, names, row, usage};

#[tokio::test]
async fn real_cli_obeys_explicit_cross_repo_keys_and_keeps_default_list_order() -> Result<()> {
    let fixture = Fixture::new(
        vec![vec![
            row("alpha", "z-ready", "idle"),
            row("beta", "a-waiting", "waiting_for_input"),
        ]],
        vec![],
    )
    .await?;
    for (args, expected) in [
        (
            vec!["status", "-a", "--sort", "state,name", "--json"],
            vec!["a-waiting", "z-ready"],
        ),
        (
            vec![
                "status", "-a", "--sort", "state", "--sort", "name", "--json",
            ],
            vec!["a-waiting", "z-ready"],
        ),
        (
            vec!["status", "-a", "--sort", "repo,state,name", "--json"],
            vec!["z-ready", "a-waiting"],
        ),
        (
            vec!["status", "-a", "--sort", "name", "--json"],
            vec!["a-waiting", "z-ready"],
        ),
        (vec!["status", "-a", "--json"], vec!["z-ready", "a-waiting"]),
        (vec!["list", "-a", "--json"], vec!["z-ready", "a-waiting"]),
    ] {
        let result = fixture.json(&args).await?;
        assert_eq!(result["schemaVersion"], 15);
        assert_eq!(names(&result), expected);
    }
    assert!(
        fixture
            .calls()
            .iter()
            .all(|call| call["method"] == "workspace.list")
    );
    Ok(())
}

#[tokio::test]
async fn real_cli_sorts_raw_resource_values_with_missing_values_last() -> Result<()> {
    let mut small = row("repo", "small", "idle");
    let mut large = row("repo", "large", "idle");
    for (value, memory) in [(&mut small, 900), (&mut large, 2048)] {
        value["runtimeResources"] = json!({"backend":"exec_server", "state":"running", "scope":"cgroup_v2", "memoryCurrentBytes":memory});
    }
    let fixture = Fixture::new(
        vec![vec![row("repo", "missing", "idle"), small, large]],
        vec![],
    )
    .await?;
    let result = fixture
        .json(&["status", "-ar", "--sort=-memory,name", "--json"])
        .await?;
    assert_eq!(names(&result), ["large", "small", "missing"]);
    assert!(
        fixture
            .calls()
            .iter()
            .all(|call| call["params"]["includeResources"] == true)
    );
    Ok(())
}

#[tokio::test]
async fn real_cli_validation_rejects_invalid_query_before_contacting_a_daemon() -> Result<()> {
    let fixture = Fixture::new(vec![vec![]], vec![]).await?;
    for args in [
        vec!["status", "--sort", "state,state"],
        vec!["status", "--sort", "memory"],
        vec!["status", "--sort", "tokens"],
        vec!["status", "--state", "closed"],
        vec!["status", "name", "--exclude-state", "unloaded"],
    ] {
        let result = fixture.run(&args).await?;
        assert!(!result.status.success());
    }
    assert!(fixture.calls().is_empty());
    Ok(())
}
