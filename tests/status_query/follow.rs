use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};

use super::*;

#[tokio::test]
async fn follow_keeps_polling_an_empty_filtered_view_and_displays_a_later_state_change()
-> Result<()> {
    let fixture = Fixture::new(
        vec![
            vec![row("repo", "agent", "not_loaded")],
            vec![row("repo", "agent", "active")],
        ],
        vec![],
    )
    .await?;
    let mut child = fixture
        .command(&[
            "status",
            "-af",
            "--exclude-state",
            "unloaded",
            "--sort",
            "state,name",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let output = tokio::time::timeout(Duration::from_secs(8), async {
        let mut output = String::new();
        while !output.contains("Working") {
            let mut line = String::new();
            anyhow::ensure!(
                stdout.read_line(&mut line).await? > 0,
                "follow exited before the next frame"
            );
            output.push_str(&line);
            anyhow::ensure!(output.len() < 65536, "unexpectedly large follow output");
        }
        Ok::<_, anyhow::Error>(output)
    })
    .await??;
    assert!(output.contains("No matching workspaces."));
    assert!(child.try_wait()?.is_none());
    child.kill().await?;
    assert!(
        fixture
            .calls()
            .iter()
            .filter(|call| call["method"] == "workspace.list")
            .count()
            >= 2
    );
    Ok(())
}

#[tokio::test]
async fn follow_reorders_across_repositories_when_attention_changes() -> Result<()> {
    let fixture = Fixture::new(
        vec![
            vec![
                row("alpha", "agent-a", "idle"),
                row("beta", "agent-b", "waiting_for_input"),
            ],
            vec![
                row("alpha", "agent-a", "waiting_for_input"),
                row("beta", "agent-b", "idle"),
            ],
        ],
        vec![],
    )
    .await?;
    let mut child = fixture
        .command(&["status", "-af", "--sort", "state,name"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let output = tokio::time::timeout(Duration::from_secs(8), async {
        let mut output = String::new();
        while agent_order(&output).len() < 4 {
            let mut line = String::new();
            anyhow::ensure!(stdout.read_line(&mut line).await? > 0, "follow exited");
            output.push_str(&line);
            anyhow::ensure!(output.len() < 65536, "unexpectedly large follow output");
        }
        Ok::<_, anyhow::Error>(output)
    })
    .await??;
    assert_eq!(
        agent_order(&output),
        ["agent-b", "agent-a", "agent-a", "agent-b"]
    );
    assert!(child.try_wait()?.is_none());
    child.kill().await?;
    Ok(())
}

fn agent_order(output: &str) -> Vec<&str> {
    output
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .filter(|name| matches!(*name, "agent-a" | "agent-b"))
        .collect()
}

#[tokio::test]
async fn follow_reorders_for_new_native_activity_without_a_state_change() -> Result<()> {
    let fixture = Fixture::new(
        vec![
            vec![
                row_with_recency("alpha", "agent-a", "idle", Some(1_000)),
                row_with_recency("beta", "agent-b", "idle", Some(2_000)),
            ],
            vec![
                row_with_recency("alpha", "agent-a", "idle", Some(3_000)),
                row_with_recency("beta", "agent-b", "idle", Some(2_000)),
            ],
        ],
        vec![],
    )
    .await?;
    let mut child = fixture
        .command(&["status", "-af", "--sort", "recent"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let output = tokio::time::timeout(Duration::from_secs(8), async {
        let mut output = String::new();
        while agent_order(&output).len() < 4 {
            let mut line = String::new();
            anyhow::ensure!(stdout.read_line(&mut line).await? > 0, "follow exited");
            output.push_str(&line);
            anyhow::ensure!(output.len() < 65536, "unexpectedly large follow output");
        }
        Ok::<_, anyhow::Error>(output)
    })
    .await??;
    assert_eq!(
        agent_order(&output),
        ["agent-b", "agent-a", "agent-a", "agent-b"]
    );
    assert!(child.try_wait()?.is_none());
    child.kill().await?;
    assert!(
        fixture
            .calls()
            .iter()
            .all(|call| call["method"] == "workspace.list")
    );
    Ok(())
}
