use super::*;

#[tokio::test]
async fn recency_is_available_in_json_without_changing_status_or_list_order_and_columns()
-> Result<()> {
    let mut old = row("alpha", "old", "idle");
    let mut recent = row("zeta", "recent", "idle");
    for (workspace, time) in [(&mut old, 1_000), (&mut recent, 1_800_000_000_000_i64)] {
        workspace["threadRuntime"] = json!({"status":{"type":"idle"},
            "recencyAtMs":time, "runtimeGeneration":"test", "observedAtMs":1,
            "isFresh":true});
        workspace["branchName"] = workspace["name"].clone();
    }
    let fixture = Fixture::new(vec![vec![recent, old]], vec![]).await?;
    for command in ["status", "list"] {
        let result = fixture.json(&[command, "-a", "--json"]).await?;
        assert_eq!(result["schemaVersion"], 18);
        assert_eq!(names(&result), ["old", "recent"]);
        assert_eq!(
            result["workspaces"][1]["threadRuntime"]["recencyAtMs"],
            1_800_000_000_000_i64
        );
        let output = fixture.run(&[command, "-a"]).await?;
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout)?;
        assert!(text.contains("BRANCH"));
        assert!(!text.contains(" ago"));
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
async fn reported_dollars_are_rendered_and_keep_estimate_provenance_in_json() -> Result<()> {
    let workspace = row("repo", "reported-cost", "idle");
    let mut observation = usage(&workspace, 100);
    observation["cost"] = json!({
        "status": "available", "estimatedUsageCreditsMicros": 1_250_000,
        "estimatedUsageUsdMicros": 420_000, "nativeUsageUsdMicros": 310_000,
        "groups": [], "observedAtMs": 1,
    });
    let fixture = Fixture::new(vec![vec![workspace]], vec![observation]).await?;
    let result = fixture.json(&["status", "-au", "--json"]).await?;
    assert_eq!(result["schemaVersion"], 18);
    let cost = &result["workspaces"][0]["usage"]["cost"];
    assert_eq!(cost["nativeUsageUsdMicros"], 310_000);
    assert_eq!(cost["estimatedUsageUsdMicros"], 420_000);
    assert_eq!(cost["estimatedUsageCreditsMicros"], 1_250_000);
    let output = fixture.run(&["status", "-au"]).await?;
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout)?;
    assert!(text.contains("$0.31"));
    assert!(!text.contains("$0.42"));
    assert_eq!(
        fixture
            .calls()
            .iter()
            .map(|call| call["method"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "workspace.list",
            "workspace.usage.list",
            "workspace.list",
            "workspace.usage.list"
        ]
    );
    Ok(())
}

#[tokio::test]
async fn exclusion_hides_only_unloaded_and_never_includes_closed() -> Result<()> {
    let fixture = Fixture::new(
        vec![vec![
            row("repo", "prepared", "prepared"),
            row("repo", "unloaded", "not_loaded"),
            row("repo", "closed", "closed"),
        ]],
        vec![],
    )
    .await?;
    let result = fixture
        .json(&["status", "-a", "--exclude-state", "unloaded", "--json"])
        .await?;
    assert_eq!(names(&result), ["prepared"]);
    assert!(
        fixture
            .calls()
            .iter()
            .all(|call| call["params"].get("phases").is_none())
    );
    Ok(())
}

#[tokio::test]
async fn usage_is_read_only_for_visible_rows_then_joined_before_sort_and_quota_is_global()
-> Result<()> {
    let low = row("repo", "low", "active");
    let high = row("repo", "high", "active");
    let hidden = row("repo", "hidden", "not_loaded");
    let fixture = Fixture::new(
        vec![vec![low.clone(), high.clone(), hidden.clone()]],
        vec![usage(&hidden, 9999), usage(&low, 900), usage(&high, 2000)],
    )
    .await?;
    let result = fixture
        .json(&[
            "status",
            "-auq",
            "--state",
            "working",
            "--sort=-tokens",
            "--json",
        ])
        .await?;
    assert_eq!(names(&result), ["high", "low"]);
    assert_eq!(result["workspaces"][0]["phase"], "active");
    assert_eq!(
        result["workspaces"][0]["usage"]["tokens"]["total"]["totalTokens"],
        2000
    );
    assert!(result.get("accountQuota").is_some());
    let calls = fixture.calls();
    assert_eq!(
        calls
            .iter()
            .filter(|call| call["method"] == "workspace.usage.get")
            .count(),
        2
    );
    assert!(
        !calls
            .iter()
            .any(|call| call["params"]["workspace"] == hidden["id"])
    );
    assert_eq!(
        calls
            .iter()
            .filter(|call| call["method"] == "account.quota.get")
            .count(),
        1
    );
    assert_eq!(calls[0]["params"]["phases"], json!(["active"]));
    Ok(())
}

#[tokio::test]
async fn filtered_tree_prunes_empty_groups_and_empty_view_skips_usage_but_keeps_quota() -> Result<()>
{
    let fixture = Fixture::new(
        vec![vec![
            row("repo", "frontend/one", "active"),
            row("repo", "frontend/two", "not_loaded"),
            row("repo", "backend/empty", "not_loaded"),
        ]],
        vec![],
    )
    .await?;
    let output = fixture
        .run(&["status", "-at", "--exclude-state", "unloaded"])
        .await?;
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout)?;
    assert!(text.contains("frontend/one"));
    assert!(!text.contains("backend"));
    assert!(!text.contains("frontend/two"));
    let empty = fixture
        .json(&["status", "-auq", "--state", "prepared", "--json"])
        .await?;
    assert_eq!(names(&empty), Vec::<&str>::new());
    assert!(empty.get("accountQuota").is_some());
    assert!(!fixture.calls().iter().any(|call| {
        call["method"]
            .as_str()
            .unwrap()
            .starts_with("workspace.usage")
    }));
    Ok(())
}

#[tokio::test]
async fn concurrent_removal_between_status_and_selected_usage_leaves_usage_unknown() -> Result<()> {
    let fixture = Fixture::new(
        vec![vec![row("repo", "removed-during-read", "active")]],
        vec![],
    )
    .await?;
    let result = fixture
        .json(&[
            "status",
            "-u",
            "--state",
            "working",
            "--sort=-tokens",
            "--json",
        ])
        .await?;
    assert_eq!(names(&result), ["removed-during-read"]);
    assert!(result["workspaces"][0].get("usage").is_none());
    Ok(())
}
