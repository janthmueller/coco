use std::collections::BTreeMap;
use std::io::{self, Write};

use anyhow::{Context, Result, bail};

use crate::domain::{DecisionPrompt, DecisionState};
use crate::paths::CocoPaths;
use crate::protocol::{
    DecisionGetParams, DecisionRespondParams, DecisionResult, DecisionSubmission,
};
use crate::rpc::RpcClient;

use super::prompt::{Choice, Interaction};

pub(super) async fn decide(
    paths: &CocoPaths,
    decision_id: String,
    explicit_choice: Option<u32>,
    interaction: &mut dyn Interaction,
) -> Result<()> {
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

    let mut prompt_output = io::stderr();
    let mut read_secret = |prompt: &str| rpassword::prompt_password(prompt);
    let submission = prompt_submission(
        &decision,
        &mut prompt_output,
        explicit_choice,
        interaction,
        &mut read_secret,
    )?;
    let result = client
        .request(DecisionRespondParams {
            decision_id,
            submission,
        })
        .await?;
    let mut output = io::stdout().lock();
    writeln!(
        output,
        "Response sent to Codex for workspace {}.",
        result.workspace.name
    )?;
    Ok(())
}

fn prompt_submission<W, F>(
    result: &DecisionResult,
    output: &mut W,
    explicit_choice: Option<u32>,
    interaction: &mut dyn Interaction,
    read_secret: &mut F,
) -> Result<DecisionSubmission>
where
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
        DecisionPrompt::Approval(prompt) => {
            prompt_approval(prompt, output, explicit_choice, interaction)
        }
        DecisionPrompt::UserInput { questions }
            if explicit_choice.is_none() && interaction.is_interactive() =>
        {
            prompt_questions(questions, output, interaction, read_secret)
        }
        DecisionPrompt::UserInput { .. } if explicit_choice.is_some() => {
            bail!("--choice can answer an approval, not a structured Codex question")
        }
        DecisionPrompt::UserInput { .. } => {
            bail!("a structured Codex question requires interactive input")
        }
    }
}

fn prompt_approval<W: Write>(
    prompt: &crate::domain::DecisionApprovalPrompt,
    output: &mut W,
    explicit_choice: Option<u32>,
    interaction: &mut dyn Interaction,
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
    let choice = if let Some(choice) = explicit_choice {
        let choice =
            usize::try_from(choice).context("approval choice does not fit this platform")?;
        if !(1..=prompt.options.len()).contains(&choice) {
            bail!(
                "approval choice must be between 1 and {}",
                prompt.options.len()
            );
        }
        choice - 1
    } else {
        if !interaction.is_interactive() {
            bail!("approval choice is required without interactive input; pass --choice <NUMBER>");
        }
        output.flush()?;
        let choices = decision_choices(&prompt.options);
        interaction.select("Choose an option", &choices)?
    };
    Ok(DecisionSubmission::Choice {
        choice: u32::try_from(choice + 1).context("too many decision options")?,
    })
}

fn prompt_questions<W, F>(
    questions: &[crate::domain::DecisionQuestion],
    output: &mut W,
    interaction: &mut dyn Interaction,
    read_secret: &mut F,
) -> Result<DecisionSubmission>
where
    W: Write,
    F: FnMut(&str) -> io::Result<String>,
{
    let mut answers = BTreeMap::new();
    for question in questions {
        writeln!(output, "\n{}\n{}", question.header, question.question)?;
        let answer = if question.options.is_empty() {
            prompt_answer(output, interaction, read_secret, question.is_secret)?
        } else {
            let mut choices = decision_choices(&question.options);
            if question.allows_other {
                choices.push(Choice::new("Other", None));
            }
            output.flush()?;
            let choice = interaction.select("Choose an option", &choices)?;
            if question.allows_other && choice == question.options.len() {
                prompt_answer(output, interaction, read_secret, question.is_secret)?
            } else {
                question.options[choice].label.clone()
            }
        };
        answers.insert(question.id.clone(), answer);
    }
    Ok(DecisionSubmission::Answers { answers })
}

fn prompt_answer<W, F>(
    output: &mut W,
    interaction: &mut dyn Interaction,
    read_secret: &mut F,
    is_secret: bool,
) -> Result<String>
where
    W: Write,
    F: FnMut(&str) -> io::Result<String>,
{
    if !is_secret {
        return interaction.text("Answer");
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

fn decision_choices(options: &[crate::domain::DecisionOption]) -> Vec<Choice> {
    options
        .iter()
        .map(|option| Choice::new(option.label.clone(), option.description.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;
    use crate::domain::{
        ContextMode, Decision, DecisionApprovalPrompt, DecisionKind, DecisionOption,
        DecisionQuestion, ProfileSnapshot, Workspace, WorkspaceLifecycle, WorkspacePhase,
        WorktreeMode,
    };
    use crate::protocol::RepositorySummary;

    #[test]
    fn returns_the_selected_option_as_a_one_based_native_choice() {
        let result = approval_fixture();
        let mut interaction = ScriptedInteraction::with_choices([1]);
        let mut output = Vec::new();
        let submission = prompt_submission(
            &result,
            &mut output,
            None,
            &mut interaction,
            &mut unexpected_secret,
        )
        .unwrap();
        assert_eq!(submission, DecisionSubmission::Choice { choice: 2 });
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("git status"));
        assert_eq!(interaction.seen_choices[0][0].label, "Approve");
        assert_eq!(interaction.seen_choices[0][1].label, "Decline");
    }

    #[test]
    fn accepts_an_explicit_one_based_approval_choice_without_prompting() {
        let result = approval_fixture();
        let mut interaction = ScriptedInteraction::default();
        let mut output = Vec::new();
        let submission = prompt_submission(
            &result,
            &mut output,
            Some(2),
            &mut interaction,
            &mut unexpected_secret,
        )
        .unwrap();

        assert_eq!(submission, DecisionSubmission::Choice { choice: 2 });
        assert!(interaction.seen_choices.is_empty());
    }

    #[test]
    fn rejects_an_explicit_approval_choice_outside_the_native_options() {
        let result = approval_fixture();
        let mut interaction = ScriptedInteraction::default();
        let error = prompt_submission(
            &result,
            &mut Vec::new(),
            Some(3),
            &mut interaction,
            &mut unexpected_secret,
        )
        .unwrap_err()
        .to_string();

        assert_eq!(error, "approval choice must be between 1 and 2");
        assert!(interaction.seen_choices.is_empty());
    }

    #[test]
    fn rejects_choice_for_a_structured_question() {
        let mut result = approval_fixture();
        result.decision.kind = DecisionKind::UserInput;
        result.decision.prompt = DecisionPrompt::UserInput {
            questions: vec![DecisionQuestion {
                id: "strategy".to_owned(),
                header: "Strategy".to_owned(),
                question: "Which strategy?".to_owned(),
                options: Vec::new(),
                allows_other: true,
                is_secret: false,
            }],
        };
        let mut interaction = ScriptedInteraction::default();
        let error = prompt_submission(
            &result,
            &mut Vec::new(),
            Some(1),
            &mut interaction,
            &mut unexpected_secret,
        )
        .unwrap_err()
        .to_string();

        assert_eq!(
            error,
            "--choice can answer an approval, not a structured Codex question"
        );
    }

    #[test]
    fn never_prompts_for_a_structured_question_without_a_terminal() {
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
        let mut interaction = NonInteractive;
        let error = prompt_submission(
            &result,
            &mut Vec::new(),
            None,
            &mut interaction,
            &mut unexpected_secret,
        )
        .unwrap_err()
        .to_string();

        assert_eq!(
            error,
            "a structured Codex question requires interactive input"
        );
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
        let mut interaction = ScriptedInteraction::with_choices([0]);
        interaction.texts.push_back("private answer".to_owned());
        let mut output = Vec::new();
        let submission = prompt_submission(
            &result,
            &mut output,
            None,
            &mut interaction,
            &mut unexpected_secret,
        )
        .unwrap();
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
        let mut interaction = ScriptedInteraction::default();
        let mut output = Vec::new();
        let mut prompts = Vec::new();
        let mut read_secret = |prompt: &str| {
            prompts.push(prompt.to_owned());
            Ok("private-token".to_owned())
        };
        let submission = prompt_submission(
            &result,
            &mut output,
            None,
            &mut interaction,
            &mut read_secret,
        )
        .unwrap();
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

    #[derive(Default)]
    struct ScriptedInteraction {
        choices: VecDeque<usize>,
        texts: VecDeque<String>,
        seen_choices: Vec<Vec<Choice>>,
    }

    impl ScriptedInteraction {
        fn with_choices(choices: impl IntoIterator<Item = usize>) -> Self {
            Self {
                choices: choices.into_iter().collect(),
                ..Self::default()
            }
        }
    }

    impl Interaction for ScriptedInteraction {
        fn is_interactive(&self) -> bool {
            true
        }

        fn select(&mut self, _title: &str, choices: &[Choice]) -> Result<usize> {
            self.seen_choices.push(choices.to_vec());
            self.choices
                .pop_front()
                .context("test did not provide a selection")
        }

        fn text(&mut self, _label: &str) -> Result<String> {
            self.texts.pop_front().context("test did not provide text")
        }
    }

    struct NonInteractive;

    impl Interaction for NonInteractive {
        fn is_interactive(&self) -> bool {
            false
        }

        fn select(&mut self, _title: &str, _choices: &[Choice]) -> Result<usize> {
            panic!("non-interactive decisions must not open a selector")
        }

        fn text(&mut self, _label: &str) -> Result<String> {
            panic!("non-interactive decisions must not request text")
        }
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
                    model_override: None,
                    effective_settings: json!({}),
                },
                lifecycle: WorkspaceLifecycle::Ready,
                thread_runtime: None,
                phase: WorkspacePhase::Unavailable,
                wait_reasons: Vec::new(),
                worktree_mode: WorktreeMode::NewBranch,
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
