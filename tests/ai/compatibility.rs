use super::*;
use crate::mcp_client::McpClient;

#[tokio::test]
async fn list_data_survives_legacy_and_modern_protocols_without_expanding_capabilities()
-> Result<()> {
    for protocol in ["2025-03-26", "2025-11-25", "2026-07-28"] {
        let fixture = Fixture::with_lifecycle(true).await?;
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_coco-mcp"));
        fixture.apply(&mut command);
        command.arg("--repository").arg(fixture.root.path());
        let mut client = if protocol == "2025-03-26" {
            McpClient::spawn(&mut command).await?
        } else {
            McpClient::spawn_with_protocol(&mut command, protocol).await?
        };
        let tools = client.request("tools/list", json!({})).await?;
        assert!(
            tools["tools"]
                .as_array()
                .unwrap()
                .iter()
                .all(|tool| { tool["annotations"]["readOnlyHint"] == true })
        );
        let wire = client
            .request(
                "tools/call",
                json!({
                    "name":"workspaces.list", "arguments":{},
                }),
            )
            .await?;
        assert_ne!(wire["isError"], true);
        assert_eq!(wire["content"][0]["type"], "text");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                wire["content"][0]["text"].as_str().unwrap()
            )?,
            json!([]),
        );
        if protocol == "2026-07-28" {
            assert_eq!(wire["structuredContent"], json!([]));
        } else {
            assert!(
                wire.get("structuredContent").is_none(),
                "legacy wire: {wire}"
            );
        }
        assert_eq!(client.call("workspaces.list", json!({})).await?, json!([]));
        let object = client
            .request(
                "tools/call",
                json!({
                    "name":"workspaces.status", "arguments":{"workspace":"reviewed-name"},
                }),
            )
            .await?;
        assert!(object["structuredContent"]["workspace"].is_object());
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                object["content"][0]["text"].as_str().unwrap()
            )?,
            object["structuredContent"],
        );
        let unsupported = client.request("prompts/list", json!({})).await.unwrap_err();
        assert!(unsupported.to_string().contains("-32601"), "{unsupported}");
        client.stop().await?;
    }
    Ok(())
}
