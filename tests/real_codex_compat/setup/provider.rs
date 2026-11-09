use super::*;
use tokio::io::AsyncReadExt;

pub(super) struct ScriptedProvider(Option<tokio::task::JoinHandle<Result<()>>>);

impl ScriptedProvider {
    pub(super) async fn start(paths: &TestPaths, responses: usize) -> Result<Self> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        fs::write(
            paths.codex_home.join("config.toml"),
            format!(
                "model_provider = 'worker-proof'\napproval_policy = 'on-request'\n\
             [model_providers.worker-proof]\nname = 'Scripted local worker proof'\n\
             base_url = 'http://{address}/v1'\nwire_api = 'responses'\n\
             requires_openai_auth = false\nsupports_websockets = false\n\
             [features]\nremote_plugin = false\nskill_mcp_dependency_install = false\n"
            ),
        )?;
        let task = tokio::spawn(async move {
            for index in 0..responses {
                let (stream, _) = listener.accept().await?;
                let mut stream = BufReader::new(stream);
                let body = read_request(&mut stream).await?;
                // The selected MCP tool must be advertised, the ungranted one
                // must not. No actual model/remote provider is contacted.
                let tools = serde_json::to_string(&body["tools"])?;
                ensure!(
                    tools.contains("run_submit"),
                    "worker submit was not advertised"
                );
                ensure!(
                    !tools.contains("run_forbidden"),
                    "ungranted tool advertised"
                );
                let grant = if index / 2 == 1 { "run-b" } else { "run-a" };
                let body = response(index, grant)?;
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.get_mut().write_all(head.as_bytes()).await?;
                stream.get_mut().write_all(body.as_bytes()).await?;
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
}

impl Drop for ScriptedProvider {
    fn drop(&mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
        }
    }
}

async fn read_request(stream: &mut BufReader<TcpStream>) -> Result<Value> {
    let mut length = None;
    loop {
        let mut line = String::new();
        ensure!(
            stream.read_line(&mut line).await? > 0,
            "HTTP request ended early"
        );
        if line == "\r\n" {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = Some(value.trim().parse::<usize>()?);
        }
    }
    let length = length.context("fixture request had no content length")?;
    ensure!(length < 2 * 1024 * 1024, "fixture request too large");
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn response(index: usize, grant: &str) -> Result<String> {
    let id = format!("response_{index}");
    let item = if index.is_multiple_of(2) {
        json!({"type": "function_call", "id": format!("fc_{index}"), "call_id": format!("call_{index}"),
            "namespace": "mcp__worker", "name": "run_submit", "arguments": serde_json::to_string(&json!({"runId": grant}))?, "status": "completed"})
    } else {
        json!({"type": "message", "id": format!("msg_{index}"), "role": "assistant", "status": "completed",
            "content": [{"type": "output_text", "text": "Fixture submitted.", "annotations": []}]})
    };
    let created = json!({"type": "response.created", "response": {"id": id, "status": "in_progress", "output": []}});
    let added = json!({"type": "response.output_item.added", "output_index": 0, "item": item});
    let done = json!({"type": "response.output_item.done", "output_index": 0, "item": item});
    let completed = json!({"type": "response.completed", "response": {"id": id, "status": "completed", "output": [item],
        "usage": {"input_tokens": 10, "output_tokens": 5, "total_tokens": 15}}});
    Ok([created, added, done, completed]
        .into_iter()
        .map(|event| {
            format!(
                "event: {}\ndata: {event}\n\n",
                event["type"].as_str().unwrap()
            )
        })
        .collect())
}
