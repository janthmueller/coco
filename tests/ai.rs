#![cfg(unix)]

use anyhow::{Result, ensure};
use serde_json::json;

#[path = "ai/host.rs"]
mod host;
#[path = "ai/support.rs"]
mod support;
use host::Host;
use support::{Fixture, create_input};

#[tokio::test]
async fn create_summary_is_authoritative_and_no_request_is_dispatched_before_acceptance()
-> Result<()> {
    let fixture = Fixture::new().await?;
    let mut host = Host::start(&fixture, true, true, true).await?;
    host.begin("workspaces.create", create_input()).await?;
    let question = host.next().await?;
    assert_eq!(question["method"], "elicitation/create");
    let review = question["params"]["message"].as_str().unwrap();
    for text in [
        "Create workspace",
        "fix/login",
        "feat/auth",
        "main",
        "prepare only",
        fixture.root.path().to_str().unwrap(),
    ] {
        assert!(review.contains(text), "{text}: {review}");
    }
    assert_eq!(
        question["params"]["requestedSchema"]["properties"]["confirm"]["default"],
        false
    );
    assert!(fixture.calls.lock().unwrap().is_empty());
    host.answer(
        &question,
        json!({"action":"accept","content":{"confirm":true}}),
    )
    .await?;
    let result = host.next().await?;
    assert_eq!(
        result["result"]["structuredContent"]["error"]["code"],
        "FIXTURE_DISPATCHED"
    );
    let calls = fixture.calls.lock().unwrap().clone();
    assert_eq!(calls[0]["method"], "workspace.setup");
    assert_eq!(
        calls[0]["params"]["repositoryPath"],
        fixture.root.path().to_str().unwrap()
    );
    assert_eq!(
        calls[0]["params"]["request"]["context"],
        create_input()["context"]
    );
    host.stop().await
}

#[tokio::test]
async fn decline_cancel_missing_false_and_malformed_answers_never_dispatch() -> Result<()> {
    for response in [
        json!({"action":"decline"}),
        json!({"action":"cancel"}),
        json!({"action":"accept"}),
        json!({"action":"accept","content":{"confirm":false}}),
        json!({"action":"accept","content":{"confirm":"true"}}),
        json!({"action":"accept","content":{"confirm":true,"extra":true}}),
    ] {
        let fixture = Fixture::new().await?;
        let mut host = Host::start(&fixture, true, true, true).await?;
        host.begin("workspaces.create", create_input()).await?;
        let question = host.next().await?;
        host.answer(&question, response).await?;
        assert_eq!(
            host.next().await?["result"]["structuredContent"]["error"]["code"],
            "CONFIRMATION_CANCELLED"
        );
        host.stop().await?;
        assert!(fixture.calls.lock().unwrap().is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn unavailable_confirmation_and_missing_write_grants_fail_closed() -> Result<()> {
    for (capability, granted) in [(false, true), (true, false)] {
        let fixture = Fixture::new().await?;
        let mut host = Host::start(&fixture, true, capability, granted).await?;
        host.begin("workspaces.create", create_input()).await?;
        let result = host.next().await?;
        if granted {
            assert_eq!(
                result["result"]["structuredContent"]["error"]["code"],
                "CONFIRMATION_UNAVAILABLE"
            );
        } else {
            ensure!(result.get("error").is_some() || result["result"]["isError"] == true);
        }
        host.stop().await?;
        assert!(fixture.calls.lock().unwrap().is_empty());
    }
    let fixture = Fixture::new().await?;
    let mut host = Host::start(&fixture, true, true, true).await?;
    host.begin("workspaces.create", create_input()).await?;
    let question = host.next().await?;
    host.reject_transport(&question).await?;
    assert_eq!(
        host.next().await?["result"]["structuredContent"]["error"]["code"],
        "CONFIRMATION_UNAVAILABLE"
    );
    host.stop().await?;
    assert!(fixture.calls.lock().unwrap().is_empty());
    Ok(())
}

#[tokio::test]
async fn send_and_interrupt_also_require_the_exact_host_confirmation() -> Result<()> {
    for (tool, input, method, review_text) in [
        (
            "workspaces.send",
            json!({"workspace":"fix/login","message":"continue\nexactly","operationId":"send-key"}),
            "turn.start",
            "continue\\nexactly",
        ),
        (
            "turns.interrupt",
            json!({"workspace":"fix/login","operationId":"send-key","mode":"bestEffort"}),
            "turn.interrupt",
            "newer turn could be interrupted",
        ),
    ] {
        let fixture = Fixture::new().await?;
        let mut host = Host::start(&fixture, true, true, true).await?;
        host.begin(tool, input).await?;
        let question = host.next().await?;
        assert!(
            question["params"]["message"]
                .as_str()
                .unwrap()
                .contains(review_text)
        );
        assert!(fixture.calls.lock().unwrap().is_empty());
        host.answer(
            &question,
            json!({"action":"accept","content":{"confirm":true}}),
        )
        .await?;
        assert_eq!(
            host.next().await?["result"]["structuredContent"]["error"]["code"],
            "FIXTURE_DISPATCHED"
        );
        assert_eq!(fixture.calls.lock().unwrap()[0]["method"], method);
        host.stop().await?;
    }
    Ok(())
}

#[tokio::test]
async fn public_noninteractive_default_is_unchanged_and_model_cannot_supply_confirmation()
-> Result<()> {
    let fixture = Fixture::new().await?;
    let mut host = Host::start(&fixture, false, false, true).await?;
    host.begin("workspaces.create", create_input()).await?;
    assert_eq!(
        host.next().await?["result"]["structuredContent"]["error"]["code"],
        "FIXTURE_DISPATCHED"
    );
    host.stop().await?;
    let fixture = Fixture::new().await?;
    let mut host = Host::start(&fixture, true, true, true).await?;
    let mut input = create_input();
    input["confirmed"] = json!(true);
    host.begin("workspaces.create", input).await?;
    let result = host.next().await?;
    assert!(result.get("error").is_some() || result["result"]["isError"] == true);
    host.stop().await?;
    assert!(fixture.calls.lock().unwrap().is_empty());
    Ok(())
}

#[tokio::test]
async fn cancelled_call_and_oversized_review_do_not_dispatch() -> Result<()> {
    let fixture = Fixture::new().await?;
    let mut host = Host::start(&fixture, true, true, true).await?;
    host.begin("workspaces.create", create_input()).await?;
    let question = host.next().await?;
    host.send(json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":2,"reason":"test cancellation"}})).await?;
    host.answer(
        &question,
        json!({"action":"accept","content":{"confirm":true}}),
    )
    .await?;
    host.stop().await?;
    assert!(fixture.calls.lock().unwrap().is_empty());
    let fixture = Fixture::new().await?;
    let mut host = Host::start(&fixture, true, true, true).await?;
    host.begin(
        "workspaces.send",
        json!({"workspace":"fix/login","message":"x".repeat(17*1024)}),
    )
    .await?;
    assert_eq!(
        host.next().await?["result"]["structuredContent"]["error"]["code"],
        "CONFIRMATION_TOO_LARGE"
    );
    host.stop().await?;
    assert!(fixture.calls.lock().unwrap().is_empty());
    Ok(())
}

#[tokio::test]
async fn nonterminal_ai_never_starts_a_native_conversation_or_queries_the_daemon() -> Result<()> {
    let fixture = Fixture::new().await?;
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_coco"));
    fixture.apply(&mut command);
    let output = command
        .args(["ai", "create something"])
        .stdin(std::process::Stdio::null())
        .output()
        .await?;
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("interactive terminal"));
    assert!(fixture.calls.lock().unwrap().is_empty());
    Ok(())
}
