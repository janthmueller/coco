use super::*;
use serde_json::Value;

fn plan() -> Value {
    json!({"workspaceId":"reviewed-uuid", "workspaceName":"reviewed-name",
        "worktreePath":"/managed/reviewed-name", "removeWorktree":true,
        "headSha":"reviewed-head", "branchName":"reviewed-branch", "threadId":"reviewed-thread",
        "threadDisposition":"delete", "deleteBranch":true, "trackedChanges":false,
        "untrackedFileCount":0, "ignoredFileCount":0, "detachedCommits":false,
        "unretainedCommitCount":0, "descendantThreadCount":0})
}

fn workspace() -> Value {
    json!({"id":"reviewed-uuid", "repositoryId":"repo-test", "name":"reviewed-name",
        "contextMode":"fresh", "context":{},
        "profile":{"name":"default", "sourceHash":"test", "effectiveSettings":{}},
        "lifecycle":"ready", "availability":"closed", "phase":"closed",
        "waitReasons":[], "threadArchived":false,
        "worktreeMode":"new_branch", "branchName":"reviewed-branch", "baseSha":"base",
        "worktreePath":"/managed/reviewed-name", "closedHeadSha":"reviewed-head",
        "createdAtMs":1, "updatedAtMs":1})
}

pub(super) fn preview_response(request: &Value) -> Option<Value> {
    match request["method"].as_str()? {
        "workspace.deleteOperation" if request["params"]["dryRun"] == true => Some(json!({
            "operationId":"delete-key", "state":"preview", "repositoryId":"repo-test",
            "repositoryPath":"/repo", "requestSha256":null, "plan":plan(), "planSha256":null,
            "errorCode":null, "createdAtMs":null, "updatedAtMs":null,
        })),
        "workspace.close" if request["params"]["dryRun"] == true => {
            let mut plan = plan();
            plan["threadDisposition"] = json!("retain");
            plan["deleteBranch"] = json!(false);
            Some(json!({"workspace": workspace(), "plan":plan, "applied":false}))
        }
        "workspace.get" => Some(json!({"workspace":workspace(),
            "git":{"observed":false,"reason":"closed"}, "openDecisions":[], "nextSequence":0})),
        _ => None,
    }
}

fn input(tool: &str) -> Value {
    if tool == "workspaces.delete" {
        json!({"workspace":"model-alias", "operationId":"delete-key"})
    } else {
        json!({"workspace":"model-alias"})
    }
}

fn is_apply(request: &Value) -> bool {
    match request["method"].as_str() {
        Some("workspace.deleteOperation" | "workspace.close") => {
            request["params"]["dryRun"] == false
        }
        Some("workspace.reopen") => true,
        _ => false,
    }
}

#[tokio::test]
async fn lifecycle_confirmation_uses_authoritative_preview_and_pins_the_apply() -> Result<()> {
    for (tool, method) in [
        ("workspaces.delete", "workspace.deleteOperation"),
        ("workspaces.close", "workspace.close"),
        ("workspaces.reopen", "workspace.reopen"),
    ] {
        let fixture = Fixture::with_lifecycle(true).await?;
        let mut host = Host::start(&fixture, true, true, true).await?;
        host.begin(tool, input(tool)).await?;
        let question = host.next().await?;
        assert_eq!(
            question["method"], "elicitation/create",
            "{tool}: {question}"
        );
        let review = question["params"]["message"].as_str().unwrap();
        for required in [
            "reviewed-uuid",
            "reviewed-name",
            "reviewed-head",
            "/managed/reviewed-name",
        ] {
            assert!(review.contains(required), "{required}: {review}");
        }
        assert!(!fixture.calls.lock().unwrap().iter().any(is_apply));
        host.answer(
            &question,
            json!({"action":"accept","content":{"confirm":true}}),
        )
        .await?;
        assert_eq!(
            host.next().await?["result"]["structuredContent"]["error"]["code"],
            "FIXTURE_DISPATCHED"
        );
        let calls = fixture.calls.lock().unwrap().clone();
        let apply = calls.iter().find(|request| is_apply(request)).unwrap();
        assert_eq!(apply["method"], method);
        if tool == "workspaces.delete" {
            assert_eq!(apply["params"]["request"]["workspace"], "model-alias");
            assert_eq!(
                apply["params"]["expectedPlanSha256"]
                    .as_str()
                    .unwrap()
                    .len(),
                64
            );
        } else {
            assert_eq!(apply["params"]["workspace"], "reviewed-uuid");
        }
        if tool == "workspaces.close" {
            assert_eq!(
                apply["params"]["expectedPlan"]["workspaceId"],
                "reviewed-uuid"
            );
        }
        host.stop().await?;
    }
    Ok(())
}

#[tokio::test]
async fn declined_or_unavailable_lifecycle_confirmation_never_applies() -> Result<()> {
    for tool in ["workspaces.delete", "workspaces.close", "workspaces.reopen"] {
        for capability in [true, false] {
            let fixture = Fixture::with_lifecycle(true).await?;
            let mut host = Host::start(&fixture, true, capability, true).await?;
            host.begin(tool, input(tool)).await?;
            if capability {
                let question = host.next().await?;
                assert_eq!(
                    question["method"], "elicitation/create",
                    "{tool}: {question}"
                );
                host.answer(&question, json!({"action":"decline"})).await?;
            }
            let response = host.next().await?;
            assert_eq!(
                response["result"]["structuredContent"]["error"]["code"],
                if capability {
                    "CONFIRMATION_CANCELLED"
                } else {
                    "CONFIRMATION_UNAVAILABLE"
                }
            );
            assert!(!fixture.calls.lock().unwrap().iter().any(is_apply));
            host.stop().await?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn lifecycle_grants_cannot_be_bypassed_by_direct_tool_calls() -> Result<()> {
    let fixture = Fixture::with_lifecycle(true).await?;
    let mut host = Host::start(&fixture, false, false, false).await?;
    for tool in ["workspaces.delete", "workspaces.close", "workspaces.reopen"] {
        host.begin(tool, input(tool)).await?;
        let response = host.next().await?;
        assert!(response.get("error").is_some() || response["result"]["isError"] == true);
    }
    assert!(fixture.calls.lock().unwrap().is_empty());
    host.stop().await
}
