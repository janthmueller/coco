//! Observe only bounded, disposable native rollouts, never the operator's history.

use super::*;
use std::collections::BTreeSet;
use std::io::Read;

pub(super) struct Progress {
    sessions: PathBuf,
    before: BTreeSet<PathBuf>,
    cwd: PathBuf,
}

impl Progress {
    pub(super) fn snapshot(paths: &TestPaths, cwd: &Path) -> Result<Self> {
        let sessions = paths.codex_home.join("sessions");
        Ok(Self {
            before: rollouts(&sessions)?,
            sessions,
            cwd: cwd.canonicalize()?,
        })
    }

    pub(super) fn completed(&self) -> Result<bool> {
        for path in rollouts(&self.sessions)?.difference(&self.before) {
            let mut bytes = Vec::new();
            fs::File::open(path)?
                .take(2 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() <= 2 * 1024 * 1024,
                "synthetic control rollout exceeded its test bound"
            );
            if completed(&bytes, &self.cwd)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn rollouts(root: &Path) -> Result<BTreeSet<PathBuf>> {
    let mut directories = vec![root.to_owned()];
    let mut files = BTreeSet::new();
    for _ in 0..4 {
        let mut next = Vec::new();
        for directory in directories {
            let entries = match fs::read_dir(directory) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            for entry in entries {
                let entry = entry?;
                let kind = entry.file_type()?;
                if kind.is_dir() {
                    next.push(entry.path());
                } else if kind.is_file()
                    && entry
                        .path()
                        .extension()
                        .is_some_and(|value| value == "jsonl")
                {
                    files.insert(entry.path());
                }
                ensure!(
                    next.len() <= 16 && files.len() <= 16,
                    "synthetic rollout inventory exceeded its test bound"
                );
            }
        }
        directories = next;
    }
    Ok(files)
}

fn completed(bytes: &[u8], cwd: &Path) -> Result<bool> {
    let records = bytes
        .split_inclusive(|byte| *byte == b'\n')
        .filter(|line| line.ends_with(b"\n"))
        .map(serde_json::from_slice::<Value>)
        .collect::<serde_json::Result<Vec<_>>>()
        .context("invalid synthetic native rollout record")?;
    if !records.iter().any(|record| {
        record["type"] == "session_meta"
            && record["payload"]["cwd"]
                .as_str()
                .is_some_and(|value| Path::new(value) == cwd)
    }) {
        return Ok(false);
    }
    let mut started = None;
    for record in &records {
        if record["type"] != "event_msg" {
            continue;
        }
        let payload = &record["payload"];
        match payload["type"].as_str() {
            Some("task_started" | "turn_started") => {
                started = payload["turn_id"].as_str();
            }
            Some("task_complete" | "turn_complete")
                if started.is_some() && started == payload["turn_id"].as_str() =>
            {
                ensure!(
                    payload["error"].is_null(),
                    "native control turn completed with an error; body withheld"
                );
                return Ok(true);
            }
            Some("turn_aborted") => bail!("native control turn was aborted"),
            _ => {}
        }
    }
    Ok(false)
}

#[test]
fn completion_uses_native_identity_not_assistant_text() -> Result<()> {
    let record = |kind, payload| format!("{}\n", json!({"type":kind,"payload":payload}));
    let meta = record("session_meta", json!({"cwd":"/synthetic/repository"}));
    let start = record("event_msg", json!({"type":"task_started","turn_id":"one"}));
    let wrong = record(
        "event_msg",
        json!({"type":"task_complete","turn_id":"other"}),
    );
    let done = record(
        "event_msg",
        json!({"type":"task_complete","turn_id":"one","last_agent_message":"any wording"}),
    );
    let cwd = Path::new("/synthetic/repository");
    assert!(!completed(format!("{meta}{start}{wrong}").as_bytes(), cwd)?);
    assert!(completed(format!("{meta}{start}{done}").as_bytes(), cwd)?);
    assert!(!completed(
        format!("{meta}{start}{done}").as_bytes(),
        Path::new("/other")
    )?);
    assert!(!completed(
        format!("{meta}{start}{{\"partial\":").as_bytes(),
        cwd
    )?);
    Ok(())
}

#[test]
fn terminal_native_errors_are_not_success_or_raw_error_output() -> Result<()> {
    let events = [
        json!({"type":"session_meta","payload":{"cwd":"/fixture"}}),
        json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"one"}}),
        json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"one","error":{"message":"synthetic-private-error"}}}),
    ];
    let bytes = events
        .iter()
        .map(|event| format!("{event}\n"))
        .collect::<String>();
    let error = completed(bytes.as_bytes(), Path::new("/fixture"))
        .unwrap_err()
        .to_string();
    assert!(!error.contains("synthetic-private-error"));
    Ok(())
}

#[test]
fn a_completed_older_session_cannot_finish_a_new_dialog() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let paths = TestPaths::new(temporary.path());
    let sessions = paths.codex_home.join("sessions");
    fs::create_dir_all(&sessions)?;
    let cwd = temporary.path().canonicalize()?;
    let events = [
        json!({"type":"session_meta","payload":{"cwd":cwd}}),
        json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"one"}}),
        json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"one"}}),
    ];
    let bytes = events
        .iter()
        .map(|event| format!("{event}\n"))
        .collect::<String>();
    fs::write(sessions.join("old.jsonl"), &bytes)?;
    let progress = Progress::snapshot(&paths, &cwd)?;
    assert!(!progress.completed()?);
    fs::write(sessions.join("new.jsonl"), &bytes)?;
    assert!(progress.completed()?);
    Ok(())
}
