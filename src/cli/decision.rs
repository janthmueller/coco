use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};

use anyhow::{Context, Result, bail};

use crate::domain::{DecisionPrompt, DecisionState};
use crate::paths::CocoPaths;
use crate::protocol::{
    DecisionGetParams, DecisionRespondParams, DecisionResult, DecisionSubmission,
};
use crate::rpc::RpcClient;

pub(super) async fn decide(paths: &CocoPaths, decision_id: String) -> Result<()> {
    let client = RpcClient::new(paths.socket_path.clone());
    let decision = client
        .request(DecisionGetParams {
            decision_id: decision_id.clone(),
        })
        .await?;
    if decision.decision.state != DecisionState::Pending {
        bail!(
            "decision {decision_id} is {}; only pending decisions can be answered",
            decision.decision.state.as_str()
        );
    }

    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let mut read_secret = |prompt: &str| rpassword::prompt_password(prompt);
    let submission = prompt_submission(&decision, &mut input, &mut output, &mut read_secret)?;
    let result = client
        .request(DecisionRespondParams {
            decision_id,
            submission,
        })
        .await?;
    writeln!(
        output,
        "Response sent to Codex for workspace {}.",
        result.workspace.name
    )?;
    Ok(())
}

fn prompt_submission<R, W, F>(
    result: &DecisionResult,
    input: &mut R,
    output: &mut W,
    read_secret: &mut F,
) -> Result<DecisionSubmission>
where
    R: BufRead,
    W: Write,
    F: FnMut(&str) -> io::Result<String>,
{
    writeln!(
        output,
        "Workspace: {} ({})",
        result.workspace.name,
        result.repository.root_path.display()
    )?;
    match &result.decision.prompt {
        DecisionPrompt::Approval(prompt) => prompt_approval(prompt, input, output),
        DecisionPrompt::UserInput { questions } => {
            prompt_questions(questions, input, output, read_secret)
        }
    }
}

fn prompt_approval<R: BufRead, W: Write>(
    prompt: &crate::domain::DecisionApprovalPrompt,
    input: &mut R,
    output: &mut W,
) -> Result<DecisionSubmission> {
    writeln!(output, "\n{}", prompt.title)?;
    if let Some(reason) = &prompt.reason {
        writeln!(output, "Reason: {reason}")?;
    }
    if let Some(command) = &prompt.command {
        writeln!(output, "Command:\n  {command}")?;
    }
    if let Some(cwd) = &prompt.cwd {
        writeln!(output, "Directory: {}", cwd.display())?;
    }
    if let Some(host) = &prompt.network_host {
        let protocol = prompt.network_protocol.as_deref().unwrap_or("network");
        writeln!(output, "Network: {protocol}://{host}")?;
    }
    if let Some(root) = &prompt.grant_root {
        writeln!(output, "Requested write root: {}", root.display())?;
    }
    for permission in &prompt.additional_permissions {
        writeln!(
            output,
            "Additional permission: {} {}",
            permission.access, permission.target
        )?;
    }
    for change in &prompt.changes {
        writeln!(output, "\n{} ({})", change.path.display(), change.kind)?;
        if !change.diff.is_empty() {
            writeln!(output, "{}", change.diff)?;
        }
        if change.diff_truncated {
            writeln!(output, "[diff truncated]")?;
        }
    }
    if prompt.options.is_empty() {
        bail!("this approval has no choices CoCo can safely send");
    }
    writeln!(output)?;
    for (index, option) in prompt.options.iter().enumerate() {
        writeln!(output, "{}. {}", index + 1, option.label)?;
        if let Some(description) = option.description.as_deref() {
            writeln!(output, "   {description}")?;
        }
    }
    let choice = prompt_number(input, output, prompt.options.len(), "Choose an option")?;
    Ok(DecisionSubmission::Choice {
        choice: u32::try_from(choice).context("too many decision options")?,
    })
}

fn prompt_questions<R, W, F>(
    questions: &[crate::domain::DecisionQuestion],
    input: &mut R,
    output: &mut W,
    read_secret: &mut F,
) -> Result<DecisionSubmission>
where
    R: BufRead,
    W: Write,
    F: FnMut(&str) -> io::Result<String>,
{
    let mut answers = BTreeMap::new();
    for question in questions {
        writeln!(output, "\n{}\n{}", question.header, question.question)?;
        let answer = if question.options.is_empty() {
            prompt_answer(input, output, read_secret, question.is_secret)?
        } else {
            for (index, option) in question.options.iter().enumerate() {
                writeln!(output, "{}. {}", index + 1, option.label)?;
                if let Some(description) = option.description.as_deref() {
                    writeln!(output, "   {description}")?;
                }
            }
            if question.allows_other {
                writeln!(output, "{}. Other", question.options.len() + 1)?;
            }
            let count = question.options.len() + usize::from(question.allows_other);
            let choice = prompt_number(input, output, count, "Choose an option")?;
            if question.allows_other && choice == count {
                prompt_answer(input, output, read_secret, question.is_secret)?
            } else {
                question.options[choice - 1].label.clone()
            }
        };
        answers.insert(question.id.clone(), answer);
    }
    Ok(DecisionSubmission::Answers { answers })
}

fn prompt_answer<R, W, F>(
    input: &mut R,
    output: &mut W,
    read_secret: &mut F,
    is_secret: bool,
) -> Result<String>
where
    R: BufRead,
    W: Write,
    F: FnMut(&str) -> io::Result<String>,
{
    if !is_secret {
        return prompt_text(input, output, "Answer");
    }
    loop {
        output.flush()?;
        let value = read_secret("Answer (hidden): ")?;
        if !value.trim().is_empty() {
            return Ok(value.trim().to_owned());
        }
        writeln!(output, "The answer cannot be empty.")?;
    }
}

fn prompt_number<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    max: usize,
    label: &str,
) -> Result<usize> {
    loop {
        write!(output, "{label} [1-{max}]: ")?;
        output.flush()?;
        let value = read_line(input)?;
        if let Ok(choice) = value.parse::<usize>()
            && (1..=max).contains(&choice)
        {
            return Ok(choice);
        }
        writeln!(output, "Enter a number from 1 to {max}.")?;
    }
}

fn prompt_text<R: BufRead, W: Write>(input: &mut R, output: &mut W, label: &str) -> Result<String> {
    loop {
        write!(output, "{label}: ")?;
        output.flush()?;
        let value = read_line(input)?;
        if !value.trim().is_empty() {
            return Ok(value.trim().to_owned());
        }
        writeln!(output, "The answer cannot be empty.")?;
    }
}

fn read_line<R: BufRead>(input: &mut R) -> Result<String> {
    let mut value = String::new();
    if input.read_line(&mut value)? == 0 {
        bail!("input ended before a choice was made");
    }
    Ok(value.trim_end_matches(['\r', '\n']).to_owned())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;
    use crate::domain::{
        ContextMode, Decision, DecisionApprovalPrompt, DecisionKind, DecisionOption,
        DecisionQuestion, ProfileSnapshot, Workspace, WorkspaceLifecycle, WorkspacePhase,
    };
    use crate::protocol::RepositorySummary;

    #[test]
    fn retries_invalid_number_and_returns_one_based_choice() {
        let result = approval_fixture();
        let mut input = io::Cursor::new(b"wat\n0\n2\n");
        let mut output = Vec::new();
        let submission =
            prompt_submission(&result, &mut input, &mut output, &mut unexpected_secret).unwrap();
        assert_eq!(submission, DecisionSubmission::Choice { choice: 2 });
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("git status"));
        assert_eq!(output.matches("Enter a number from 1 to 2.").count(), 2);
    }

    #[test]
    fn collects_listed_and_free_text_answers_without_serializing_them_to_output() {
        let mut result = approval_fixture();
        result.decision.kind = DecisionKind::UserInput;
        result.decision.prompt = DecisionPrompt::UserInput {
            questions: vec![
                DecisionQuestion {
                    id: "strategy".to_owned(),
                    header: "Strategy".to_owned(),
                    question: "Which strategy?".to_owned(),
                    options: vec![DecisionOption {
                        label: "Safe".to_owned(),
                        description: Some("Prefer safety".to_owned()),
                    }],
                    allows_other: false,
                    is_secret: false,
                },
                DecisionQuestion {
                    id: "note".to_owned(),
                    header: "Note".to_owned(),
                    question: "Anything else?".to_owned(),
                    options: Vec::new(),
                    allows_other: true,
                    is_secret: false,
                },
            ],
        };
        let mut input = io::Cursor::new(b"1\nprivate answer\n");
        let mut output = Vec::new();
        let submission =
            prompt_submission(&result, &mut input, &mut output, &mut unexpected_secret).unwrap();
        assert_eq!(
            submission,
            DecisionSubmission::Answers {
                answers: BTreeMap::from([
                    ("note".to_owned(), "private answer".to_owned()),
                    ("strategy".to_owned(), "Safe".to_owned()),
                ])
            }
        );
        assert!(
            !String::from_utf8(output)
                .unwrap()
                .contains("private answer")
        );
    }

    #[test]
    fn collects_secret_answers_without_echo_or_jump_fallback() {
        let mut result = approval_fixture();
        result.decision.kind = DecisionKind::UserInput;
        result.decision.prompt = DecisionPrompt::UserInput {
            questions: vec![DecisionQuestion {
                id: "token".to_owned(),
                header: "Token".to_owned(),
                question: "Enter a token".to_owned(),
                options: Vec::new(),
                allows_other: true,
                is_secret: true,
            }],
        };
        let mut input = io::Cursor::new(b"visible-input-must-not-be-read\n");
        let mut output = Vec::new();
        let mut prompts = Vec::new();
        let mut read_secret = |prompt: &str| {
            prompts.push(prompt.to_owned());
            Ok("private-token".to_owned())
        };
        let submission =
            prompt_submission(&result, &mut input, &mut output, &mut read_secret).unwrap();
        assert_eq!(
            submission,
            DecisionSubmission::Answers {
                answers: BTreeMap::from([("token".to_owned(), "private-token".to_owned())])
            }
        );
        assert_eq!(prompts, ["Answer (hidden): "]);
        let output = String::from_utf8(output).unwrap();
        assert!(!output.contains("private-token"));
        assert!(!output.contains("coco jump"));
    }

    fn approval_fixture() -> DecisionResult {
        DecisionResult {
            decision: Decision {
                id: "decision-1".to_owned(),
                workspace_id: "workspace-1".to_owned(),
                turn_id: Some("turn-1".to_owned()),
                kind: DecisionKind::CommandApproval,
                state: DecisionState::Pending,
                prompt: DecisionPrompt::Approval(Box::new(DecisionApprovalPrompt {
                    title: "Run command".to_owned(),
                    reason: Some("test".to_owned()),
                    command: Some("git status".to_owned()),
                    cwd: Some(PathBuf::from("/repo")),
                    network_host: None,
                    network_protocol: None,
                    grant_root: None,
                    additional_permissions: Vec::new(),
                    changes: Vec::new(),
                    options: vec![
                        DecisionOption {
                            label: "Approve".to_owned(),
                            description: None,
                        },
                        DecisionOption {
                            label: "Decline".to_owned(),
                            description: None,
                        },
                    ],
                })),
                received_at_ms: 1,
                submitted_at_ms: None,
                resolved_at_ms: None,
            },
            workspace: Workspace {
                id: "workspace-1".to_owned(),
                create_operation_id: None,
                repository_id: "repo-1".to_owned(),
                name: "fix/test".to_owned(),
                context_mode: ContextMode::Fresh,
                context: json!({}),
                profile: ProfileSnapshot {
                    name: "default".to_owned(),
                    source_path: None,
                    source_hash: "test".to_owned(),
                    effective_settings: json!({}),
                },
                lifecycle: WorkspaceLifecycle::Ready,
                thread_runtime: None,
                phase: WorkspacePhase::Unavailable,
                wait_reasons: Vec::new(),
                branch_name: Some("coco/fix/test".to_owned()),
                base_sha: Some("abc".to_owned()),
                worktree_path: Some(PathBuf::from("/worktree")),
                codex_thread_id: Some("thread-1".to_owned()),
                parent_thread_id: None,
                active_turn_id: Some("turn-1".to_owned()),
                last_error_code: None,
                last_error_message: None,
                created_at_ms: 1,
                updated_at_ms: 1,
                completed_at_ms: None,
            },
            repository: RepositorySummary {
                id: "repo-1".to_owned(),
                display_name: "repo".to_owned(),
                root_path: PathBuf::from("/repo"),
            },
        }
    }

    fn unexpected_secret(_: &str) -> io::Result<String> {
        panic!("this fixture must not request secret input")
    }
}
