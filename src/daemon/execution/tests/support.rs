use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf};

use super::*;

pub(super) const TEST_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) struct Fixture {
    pub(super) executors: WorkspaceExecutors,
    directory: tempfile::TempDir,
    client: CodexClient,
    reader: BufReader<ReadHalf<DuplexStream>>,
    writer: WriteHalf<DuplexStream>,
    _events: tokio::sync::mpsc::Receiver<crate::codex::CodexEvent>,
}

impl Fixture {
    pub(super) async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let (client, events, server) = CodexClient::test_io_pair(4096).await;
        let executors = WorkspaceExecutors::new(
            client.clone(),
            PathBuf::from("/bin/sh"),
            None,
            WorkspaceContainment::process_tree_for_test(),
            HashMap::new(),
        );
        let (reader, writer) = tokio::io::split(server);
        Self {
            executors,
            directory,
            client,
            reader: BufReader::new(reader),
            writer,
            _events: events,
        }
    }

    pub(super) fn cwd(&self, workspace: &str) -> PathBuf {
        let cwd = self.directory.path().join(workspace);
        std::fs::create_dir_all(&cwd).unwrap();
        // The existing shell reads CoCo's first argv (exec-server) as a script
        // in this directory, avoiding direct execution of freshly written files.
        let script = cwd.join("exec-server");
        if !script.exists() {
            std::fs::write(
                script,
                concat!(
                    "printf '%s\\n' \"$$\" > executor.pid\n",
                    "while [ -f start.block ]; do sleep 0.02; done\n",
                    "printf '%s\\n' 'ws://127.0.0.1:43123'\n",
                    "exec sleep 3600\n",
                ),
            )
            .unwrap();
        }
        cwd
    }

    pub(super) fn activate(
        &self,
        workspace: &str,
    ) -> JoinHandle<Result<WorkerExecutionEnvironment, WorkspaceExecutionError>> {
        let executors = self.executors.clone();
        let cwd = self.cwd(workspace);
        let workspace = workspace.to_owned();
        tokio::spawn(async move { executors.ensure(&workspace, &cwd).await })
    }

    pub(super) async fn pid(&self, workspace: &str) -> u32 {
        let path = self.cwd(workspace).join("executor.pid");
        timeout(TEST_TIMEOUT, async {
            loop {
                if let Ok(pid) = tokio::fs::read_to_string(&path).await
                    && let Ok(pid) = pid.trim().parse()
                {
                    return pid;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("fake executor did not publish its PID")
    }

    pub(super) async fn assert_stopped(pid: u32) {
        let probe = tokio::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .unwrap();
        assert!(
            !probe.success(),
            "workspace executor {pid} survived cleanup"
        );
    }

    pub(super) async fn next_request(
        &mut self,
        method: &str,
        workspace: &str,
    ) -> serde_json::Value {
        let mut line = String::new();
        let read = timeout(TEST_TIMEOUT, self.reader.read_line(&mut line))
            .await
            .expect("executor operation did not reach the App Server")
            .unwrap();
        assert_ne!(read, 0, "App Server transport closed");
        let request: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(request["method"], method);
        assert_eq!(
            request["params"]["environmentId"],
            environment_id(workspace)
        );
        request
    }

    pub(super) async fn respond(&mut self, request: &serde_json::Value) {
        self.write_response(json!({"id": request["id"], "result": {}}))
            .await;
    }

    pub(super) async fn reject(&mut self, request: &serde_json::Value) {
        self.write_response(json!({
            "id": request["id"],
            "error": {"code": -32000, "message": "test registration rejected"}
        }))
        .await;
    }

    async fn write_response(&mut self, response: serde_json::Value) {
        let mut bytes = serde_json::to_vec(&response).unwrap();
        bytes.push(b'\n');
        self.writer.write_all(&bytes).await.unwrap();
    }

    pub(super) async fn complete_activation(&mut self, workspace: &str) {
        let add = self.next_request("environment/add", workspace).await;
        self.respond(&add).await;
        let info = self.next_request("environment/info", workspace).await;
        self.respond(&info).await;
    }

    pub(super) async fn finish(&self) {
        timeout(TEST_TIMEOUT, self.executors.close())
            .await
            .expect("workspace executor cleanup timed out");
        self.client.close().await.unwrap();
    }

    pub(super) fn assert_no_more_requests(&mut self) {
        let mut line = String::new();
        assert!(futures_util::FutureExt::now_or_never(self.reader.read_line(&mut line)).is_none());
    }
}

pub(super) async fn finish_activation(
    handle: JoinHandle<Result<WorkerExecutionEnvironment, WorkspaceExecutionError>>,
) -> WorkerExecutionEnvironment {
    timeout(TEST_TIMEOUT, handle)
        .await
        .expect("workspace activation did not complete")
        .unwrap()
        .unwrap()
}
