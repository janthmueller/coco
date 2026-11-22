use super::*;
use tokio::io::AsyncReadExt;
use tokio::sync::oneshot;

pub(super) struct Provider {
    pub address: SocketAddr,
    finish: Option<oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<Result<usize>>>,
}

impl Provider {
    pub async fn start() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let (finish, mut finished) = oneshot::channel();
        let task = tokio::spawn(async move {
            let mut requests = 0;
            loop {
                let (stream, _) = tokio::select! {
                    _ = &mut finished => return Ok(requests),
                    accepted = listener.accept() => accepted?,
                };
                timeout(COMPATIBILITY_TIMEOUT, respond(stream)).await??;
                requests += 1;
            }
        });
        Ok(Self {
            address,
            finish: Some(finish),
            task: Some(task),
        })
    }

    pub async fn finish(mut self, expected: usize) -> Result<()> {
        let _ = self.finish.take().unwrap().send(());
        let requests = timeout(COMPATIBILITY_TIMEOUT, self.task.as_mut().unwrap()).await???;
        ensure!(
            requests == expected,
            "unexpected provider requests: {requests} instead of {expected}"
        );
        Ok(())
    }

    pub async fn watch<T>(
        &mut self,
        future: impl std::future::Future<Output = Result<T>>,
    ) -> Result<T> {
        tokio::select! {
            result = timeout(COMPATIBILITY_TIMEOUT, future) => {
                result.context("scripted provider operation timed out")?
            },
            finished = self.task.as_mut().unwrap() => {
                finished??;
                bail!("scripted provider unexpectedly stopped");
            },
        }
    }
}

impl Drop for Provider {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

async fn respond(stream: TcpStream) -> Result<()> {
    let mut stream = BufReader::new(stream);
    let mut line = String::new();
    stream.read_line(&mut line).await?;
    ensure!(
        line.starts_with("POST /v1/responses "),
        "unexpected scripted provider endpoint"
    );
    let mut length = None;
    loop {
        line.clear();
        ensure!(
            stream.read_line(&mut line).await? > 0,
            "truncated HTTP headers"
        );
        if line == "\r\n" {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = Some(value.trim().parse::<usize>()?);
        }
    }
    let length = length.context("scripted request length missing")?;
    ensure!(length < 2 * 1024 * 1024, "scripted request too large");
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await?;
    let request: Value = serde_json::from_slice(&body)?;
    ensure!(
        request["model"] == "provider-proof",
        "unexpected fixture model: {}",
        request["model"]
    );
    ensure!(
        request["input"].to_string().contains(MESSAGE),
        "expected fixture message missing"
    );
    let item = json!({"type":"message", "id":"provider-message", "role":"assistant", "status":"completed",
        "content":[{"type":"output_text", "text":"Saved provider proof completed.", "annotations":[]}]});
    let events = [
        json!({"type":"response.created", "response":{"id":"provider-response", "status":"in_progress", "output":[]}}),
        json!({"type":"response.output_item.added", "output_index":0, "item":item}),
        json!({"type":"response.output_item.done", "output_index":0, "item":item}),
        json!({"type":"response.completed", "response":{"id":"provider-response", "status":"completed", "output":[item],
            "usage":{"input_tokens":10,"output_tokens":5,"total_tokens":15}}}),
    ];
    let response = events
        .iter()
        .map(|event| {
            format!(
                "event: {}\ndata: {event}\n\n",
                event["type"].as_str().unwrap()
            )
        })
        .collect::<String>();
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.len()
    );
    stream.get_mut().write_all(header.as_bytes()).await?;
    stream.get_mut().write_all(response.as_bytes()).await?;
    stream.get_mut().shutdown().await?;
    Ok(())
}
