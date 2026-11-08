use super::*;
use serde_json::Value;

pub(super) struct Source {
    pub(super) workspace_id: String,
    pub(super) thread_id: String,
    workspace: Value,
    worktree: PathBuf,
    head: Vec<u8>,
}

const NAME: &str = "source/context";
const README: &[u8] = b"original workspace working changes\n";
const UNTRACKED: &[u8] = b"original untracked content\n";

impl Source {
    pub(super) async fn create(
        paths: &TestPaths,
        binary: &Path,
        repository: &Path,
        context: &str,
    ) -> Result<Self> {
        run_cli(
            paths,
            binary,
            repository,
            &["create", NAME, "--context", context],
        )
        .await?;
        let before = workspace_status_for(paths, binary, repository, NAME).await?;
        let workspace = before["workspace"].clone();
        let workspace_id = workspace["id"]
            .as_str()
            .context("source workspace missing")?
            .to_owned();
        let thread_id = workspace["codexThreadId"]
            .as_str()
            .context("source thread missing")?
            .to_owned();
        let worktree = PathBuf::from(
            workspace["worktreePath"]
                .as_str()
                .context("source worktree missing")?,
        );
        // Context-only inheritance must leave even dirty source files alone.
        fs::write(worktree.join("README.md"), README)?;
        fs::write(worktree.join("original-untracked.txt"), UNTRACKED)?;
        let head = git(&worktree, &["rev-parse", "HEAD"])?;
        Ok(Self {
            workspace_id,
            thread_id,
            workspace,
            worktree,
            head,
        })
    }

    pub(super) async fn verify(
        &self,
        paths: &TestPaths,
        binary: &Path,
        repository: &Path,
    ) -> Result<()> {
        let after = workspace_status_for(paths, binary, repository, NAME).await?;
        for key in [
            "id",
            "name",
            "availability",
            "branchName",
            "baseSha",
            "codexThreadId",
            "parentThreadId",
            "worktreePath",
        ] {
            ensure!(
                after["workspace"][key] == self.workspace[key],
                "source {key} changed"
            );
        }
        ensure!(
            fs::read(self.worktree.join("README.md"))? == README,
            "source tracked changes lost"
        );
        ensure!(
            fs::read(self.worktree.join("original-untracked.txt"))? == UNTRACKED,
            "source untracked content lost"
        );
        ensure!(
            git(&self.worktree, &["rev-parse", "HEAD"])? == self.head,
            "source commit changed"
        );
        let branch = self.workspace["branchName"]
            .as_str()
            .context("source branch missing")?;
        ensure!(
            git(&self.worktree, &["symbolic-ref", "--short", "HEAD"])?
                == format!("{branch}\n").as_bytes(),
            "source branch changed"
        );
        let mut native =
            RealAppServer::spawn(paths, binary, repository, "temporary-source-preserved").await?;
        let history = native
            .request(
                "thread/read",
                serde_json::json!({
                    "threadId": self.thread_id, "includeTurns": true
                }),
            )
            .await?;
        ensure!(
            history["thread"]["id"] == self.thread_id,
            "source conversation changed"
        );
        let path = history["thread"]["path"]
            .as_str()
            .context("source history missing")?;
        ensure!(
            fs::canonicalize(path)?
                .starts_with(fs::canonicalize(paths.codex_home.join("sessions"))?),
            "source conversation archived or moved"
        );
        ensure!(
            history["thread"]["turns"]
                .to_string()
                .contains("coco-inherited-temporary-context"),
            "source history lost"
        );
        native.stop().await
    }
}

fn git(worktree: &Path, arguments: &[&str]) -> Result<Vec<u8>> {
    let output = std::process::Command::new("git")
        .arg("--no-optional-locks")
        .args(arguments)
        .current_dir(worktree)
        .output()?;
    ensure!(output.status.success(), "source Git inspection failed");
    Ok(output.stdout)
}
