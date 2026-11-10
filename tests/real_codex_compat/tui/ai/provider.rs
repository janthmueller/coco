use super::*;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

pub(super) struct Provider(Option<tokio::task::JoinHandle<Result<()>>>);

impl Provider {
    pub(super) async fn start(paths: &TestPaths) -> Result<Self> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let forbidden = paths.data_dir.join("inherited-mcp-started");
        fs::write(
            paths.codex_home.join("config.toml"),
            format!(
                "model_provider='ai-fixture'\napproval_policy='never'\napprovals_reviewer='user'\ncheck_for_update_on_startup=false\n[analytics]\nenabled=false\n\
             [model_providers.ai-fixture]\nname='Local scripted AI fixture'\nbase_url='http://{address}/v1'\n\
             wire_api='responses'\nrequires_openai_auth=false\nsupports_websockets=false\n\
             [features]\nshell_tool=true\nconnectors=true\nmemory_tool=true\nremote_plugin=false\nskill_mcp_dependency_install=false\n\
             [mcp_servers.inherited]\ncommand='/bin/sh'\nrequired=true\nargs=['-c', {}]\n",
                toml::Value::String(format!("touch {}; exit 1", shell_word(&forbidden))),
            ),
        )?;
        let task = tokio::spawn(async move {
            for index in 0..7 {
                let (stream, _) = timeout(COMPATIBILITY_TIMEOUT, listener.accept()).await??;
                let mut stream = BufReader::new(stream);
                let body = timeout(COMPATIBILITY_TIMEOUT, read_body(&mut stream)).await??;
                if matches!(index, 1 | 3 | 4) {
                    let results = body["input"]
                        .as_array()
                        .context("input missing")?
                        .iter()
                        .filter(|item| {
                            item.get("call_id").is_some() && item.get("output").is_some()
                        })
                        .map(|item| item["output"].clone())
                        .collect::<Vec<_>>();
                    ensure!(
                        results
                            .iter()
                            .any(|value| value.to_string().contains("configured")),
                        "native tool did not configure a workspace: {}",
                        serde_json::to_string(&results)?
                    );
                }
                if index == 6 {
                    let output = body["input"]
                        .as_array()
                        .context("input missing")?
                        .iter()
                        .filter_map(|item| item.get("output"))
                        .map(Value::to_string)
                        .collect::<String>();
                    ensure!(
                        output.contains("human-approved"),
                        "read-only tool did not return the prepared workspace: {output}"
                    );
                }
                let item = match index {
                    0 => call(&body, 1, "human-approved", "ai-approval-check")?,
                    2 => call(&body, 1, "auto-created", "ai-auto-frontend")?,
                    3 => call(&body, 2, "auto-created", "ai-auto-backend")?,
                    5 => readonly_call(&body)?,
                    _ => {
                        json!({"type":"message", "id":format!("msg-{index}"), "role":"assistant", "status":"completed",
                        "content":[{"type":"output_text", "text":match index { 1 => "AI approval fixture finished", 6 => "AI readonly fixture finished", _ => "AI multi-repository fixture finished" }, "annotations":[]}]})
                    }
                };
                let response = events(index, item);
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    response.len()
                );
                stream.get_mut().write_all(header.as_bytes()).await?;
                stream.get_mut().write_all(response.as_bytes()).await?;
                stream.get_mut().shutdown().await?;
            }
            Ok(())
        });
        Ok(Self(Some(task)))
    }

    pub(super) async fn finish(mut self) -> Result<()> {
        timeout(COMPATIBILITY_TIMEOUT, self.0.as_mut().unwrap()).await???;
        Ok(())
    }

    pub(super) fn is_finished(&self) -> bool {
        self.0.as_ref().is_some_and(|task| task.is_finished())
    }
}

impl Drop for Provider {
    fn drop(&mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
        }
    }
}

async fn read_body(stream: &mut BufReader<TcpStream>) -> Result<Value> {
    let mut length = None;
    loop {
        let mut line = String::new();
        ensure!(
            stream.read_line(&mut line).await? > 0,
            "incomplete fixture request"
        );
        if line == "\r\n" {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = Some(value.trim().parse::<usize>()?);
        }
    }
    let length = length.context("fixture request length missing")?;
    ensure!(length <= 4 * 1024 * 1024, "fixture request too large");
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await?;
    Ok(serde_json::from_slice(&body)?)
}

fn call(body: &Value, repository: usize, name: &str, operation: &str) -> Result<Value> {
    let prefix = format!("mcp__coco_repo_{repository}_");
    let arguments = json!({"operationId":operation, "name":name, "worktree":{"kind":"newBranch", "base":{"kind":"revision", "revision":"HEAD"}}, "changes":"reject", "profile":"default"});
    // Some native models expose deferred tools through the sandboxed V8
    // orchestrator rather than legacy top-level schemas. Exercise that path.
    if body.get("tools").is_none() {
        let input = format!(
            r#"
const forbidden = ALL_TOOLS.filter(t => /(^|__)(exec_command|write_stdin|spawn_agent)$/.test(t.name) || t.name.startsWith('mcp__inherited'));
if (forbidden.length) throw new Error('Forbidden AI execution tools: ' + JSON.stringify(forbidden.map(t => t.name)));
const selected = ALL_TOOLS.find(t => t.name.startsWith({prefix}) && t.name.endsWith('__workspaces_create'));
if (!selected) throw new Error('Scoped create tool missing: ' + JSON.stringify(ALL_TOOLS.map(t => t.name)));
text(await tools[selected.name]({arguments}));
"#,
            prefix = serde_json::to_string(&prefix)?,
            arguments = arguments
        );
        return Ok(
            json!({"type":"custom_tool_call", "id":format!("fc-{operation}"), "call_id":format!("call-{operation}"),
            "namespace":"functions", "name":"exec", "input":input, "status":"completed"}),
        );
    }
    let namespace = body["tools"]
        .as_array()
        .context("tool list missing")?
        .iter()
        .find_map(|tool| {
            tool["name"]
                .as_str()
                .filter(|name| name.starts_with(&prefix))
        })
        .with_context(|| {
            format!(
                "repository {repository} MCP namespace missing: {}",
                body["tools"]
            )
        })?;
    Ok(
        json!({"type":"function_call", "id":format!("fc-{operation}"), "call_id":format!("call-{operation}"),
        "namespace":namespace, "name":"workspaces_create", "status":"completed",
        "arguments":serde_json::to_string(&arguments)?}),
    )
}

fn readonly_call(body: &Value) -> Result<Value> {
    if let Some(tools) = body["tools"].as_array() {
        ensure!(
            !body["tools"].to_string().contains("workspaces_create"),
            "read-only create advertised"
        );
        let namespace = tools
            .iter()
            .find_map(|tool| {
                tool["name"]
                    .as_str()
                    .filter(|name| name.starts_with("mcp__coco_repo_1_"))
            })
            .context("read-only native namespace missing")?;
        return Ok(
            json!({"type":"function_call", "id":"fc-readonly", "call_id":"call-readonly", "namespace":namespace,
            "name":"workspaces_list", "arguments":"{}", "status":"completed"}),
        );
    }
    let input = r#"
const changing = ALL_TOOLS.filter(t => /__(workspaces_create|workspaces_send|turns_interrupt)$/.test(t.name));
if (changing.length) throw new Error('Read-only mutation tools exposed');
const listing = ALL_TOOLS.find(t => t.name.startsWith('mcp__coco_repo_1_') && t.name.endsWith('__workspaces_list'));
if (!listing) throw new Error('Read-only listing missing');
text(await tools[listing.name]({}));
"#;
    Ok(
        json!({"type":"custom_tool_call", "id":"fc-readonly", "call_id":"call-readonly", "namespace":"functions",
        "name":"exec", "input":input, "status":"completed"}),
    )
}

fn events(index: usize, item: Value) -> String {
    let id = format!("response-{index}");
    [json!({"type":"response.created", "response":{"id":id, "status":"in_progress", "output":[]}}),
        json!({"type":"response.output_item.added", "output_index":0, "item":item}),
        json!({"type":"response.output_item.done", "output_index":0, "item":item}),
        json!({"type":"response.completed", "response":{"id":id, "status":"completed", "output":[item],
            "usage":{"input_tokens":10, "output_tokens":5, "total_tokens":15}}})]
        .into_iter().map(|value| format!("event: {}\ndata: {value}\n\n", value["type"].as_str().unwrap())).collect()
}
