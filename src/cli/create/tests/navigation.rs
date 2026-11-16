use super::*;
use crate::cli::prompt::{Navigation, NavigationExit, NavigationOptions};

mod carry;
mod dependencies;
mod memory;
mod regressions;
mod safety;

enum Action {
    Pick(&'static str, &'static str),
    Enter(&'static str),
    Tab(&'static str),
    Back(&'static str),
    Cancel(&'static str),
    Text(&'static str, &'static str),
    Retry(&'static str),
    Fail(&'static str),
}

use Action::*;

struct Dialog {
    actions: VecDeque<Action>,
    focus: Vec<(String, Option<String>)>,
    back_available: Vec<(String, bool)>,
    initial_text: Vec<(String, String)>,
    reviews: Vec<Vec<ReviewField>>,
    notices: Vec<String>,
}

impl Dialog {
    fn new(actions: impl IntoIterator<Item = Action>) -> Self {
        Self {
            actions: actions.into_iter().collect(),
            focus: Vec::new(),
            back_available: Vec::new(),
            initial_text: Vec::new(),
            reviews: Vec::new(),
            notices: Vec::new(),
        }
    }

    fn consumed(&self) {
        assert!(self.actions.is_empty(), "unconsumed dialog actions");
    }

    fn value(&self, label: &str) -> &str {
        &self
            .reviews
            .last()
            .unwrap()
            .iter()
            .find(|field| field.label == label)
            .unwrap()
            .value
    }
}

impl Interaction for Dialog {
    fn is_interactive(&self) -> bool {
        true
    }
    fn select(&mut self, _: &str, _: &[Choice]) -> Result<usize> {
        panic!("navigation adapter required")
    }
    fn select_with_review(&mut self, _: &str, _: &[ReviewField], _: &[Choice]) -> Result<usize> {
        panic!("navigation adapter required")
    }
    fn confirm(&mut self, _: &str) -> Result<bool> {
        panic!("no consent reuse")
    }
    fn text(&mut self, _: &str) -> Result<String> {
        panic!("navigation adapter required")
    }
    fn notice(&mut self, message: &str) -> Result<()> {
        self.notices.push(message.into());
        Ok(())
    }

    fn select_navigable(
        &mut self,
        title: &str,
        fields: &[ReviewField],
        choices: &[Choice],
        options: NavigationOptions,
    ) -> Result<Navigation<StepSelection>> {
        self.back_available.push((title.into(), options.allow_back));
        self.focus.push((
            title.into(),
            options
                .selected
                .map(|index| choices[index].identity().to_owned()),
        ));
        if !fields.is_empty() {
            self.reviews.push(fields.to_vec());
        }
        match self
            .actions
            .pop_front()
            .expect("test needs next dialog action")
        {
            Pick(expected, label) => {
                assert_eq!(title, expected);
                let index = choices
                    .iter()
                    .position(|choice| choice.label == label || choice.identity() == label)
                    .unwrap_or_else(|| panic!("{title} missing {label}: {choices:?}"));
                Ok(Navigation::Value(StepSelection::Selected(index)))
            }
            Enter(expected) => {
                assert_eq!(title, expected);
                let index = options
                    .selected
                    .or_else(|| choices.iter().position(|choice| choice.is_default))
                    .unwrap_or(0);
                Ok(Navigation::Value(StepSelection::Selected(index)))
            }
            Tab(expected) => {
                assert_eq!(title, expected);
                assert!(options.allow_defaults);
                Ok(Navigation::Value(StepSelection::UseDefaults))
            }
            Back(expected) => {
                assert_eq!(title, expected);
                assert!(options.allow_back);
                Ok(Navigation::Back)
            }
            Cancel(expected) => {
                assert_eq!(title, expected);
                Ok(Navigation::Cancel)
            }
            Fail(expected) => {
                assert_eq!(title, expected);
                anyhow::bail!("synthetic prompt failure");
            }
            Text(_, _) | Retry(_) => panic!("expected text input or recovery, got picker {title}"),
        }
    }

    fn text_navigable(
        &mut self,
        label: &str,
        initial: &str,
        back: bool,
    ) -> Result<Navigation<String>> {
        self.initial_text.push((label.into(), initial.into()));
        match self
            .actions
            .pop_front()
            .expect("test needs next text action")
        {
            Text(expected, value) => {
                assert_eq!(label, expected);
                Ok(Navigation::Value(value.into()))
            }
            Back(expected) => {
                assert_eq!(label, expected);
                assert!(back);
                Ok(Navigation::Back)
            }
            Cancel(expected) => {
                assert_eq!(label, expected);
                Ok(Navigation::Cancel)
            }
            _ => panic!("expected picker, got text input {label}"),
        }
    }

    fn recover(&mut self, title: &str, error: anyhow::Error, _: bool) -> Result<Navigation<()>> {
        match self.actions.front() {
            Some(Retry(expected)) if *expected == title => {
                self.actions.pop_front();
                self.notices.push(error.to_string());
                Ok(Navigation::Value(()))
            }
            Some(Back(expected)) if *expected == title => {
                self.actions.pop_front();
                self.notices.push(error.to_string());
                Ok(Navigation::Back)
            }
            _ => Err(error),
        }
    }
}

async fn run_dialog(args: CreateArgs, dialog: &mut Dialog) -> CreateArgs {
    let result = walkthrough(&test_paths(), "/repo".into(), args, dialog)
        .await
        .unwrap();
    dialog.consumed();
    result
}

fn named() -> CreateArgs {
    create_args(&["coco", "create", "review/api", "-i"])
}

#[tokio::test]
async fn nested_branch_and_text_back_return_to_their_parent() {
    let mut dialog = Dialog::new([
        Pick("Worktree", "Custom branch"),
        Back("Branch name"),
        Pick("Worktree", "New branch"),
        Pick("Code", "Branch"),
        Back("Code from branch"),
        Pick("Code", "Other revision"),
        Back("Git revision"),
        Pick("Code", "Current checkout"),
        Tab("Git changes"),
        Pick("Create workspace", "Create"),
    ]);
    let args = run_dialog(named(), &mut dialog).await;
    assert!(args.branch.is_none() && args.base.is_none());
    assert_eq!(dialog.value("Code"), "main · aaaaaaaa");
}

#[tokio::test]
async fn previous_step_prefills_name_and_derived_branch_tracks_it() {
    let mut dialog = Dialog::new([
        Text("Workspace name", "first"),
        Back("Worktree"),
        Text("Workspace name", "second"),
        Tab("Worktree"),
        Pick("Create workspace", "Create"),
    ]);
    let args = run_dialog(create_args(&["coco", "create"]), &mut dialog).await;
    assert_eq!(args.name.as_deref(), Some("second"));
    assert_eq!(
        dialog.initial_text[1],
        ("Workspace name".into(), "first".into())
    );
    assert_eq!(dialog.value("Worktree"), "branch second");
}

#[tokio::test]
async fn back_after_tab_stops_automatic_advancement_and_keeps_later_answers() {
    let mut dialog = Dialog::new([
        Tab("Worktree"),
        Back("Create workspace"),
        Pick("Worktree", "Custom branch"),
        Text("Branch name", "custom"),
        Pick("Create workspace", "Create"),
    ]);
    let args = run_dialog(named(), &mut dialog).await;
    assert_eq!(args.branch.as_deref(), Some("custom"));
    assert!(args.context.is_none() && !args.jump);
}

#[tokio::test]
async fn edit_name_returns_directly_to_fresh_review_and_preserves_explicit_branch() {
    let args = create_args(&["coco", "create", "review/api", "-i", "--branch", "explicit"]);
    let mut dialog = Dialog::new([
        Tab("Code"),
        Pick("Create workspace", "Edit settings"),
        Pick("Edit settings", "Workspace"),
        Text("Workspace name", "renamed"),
        Pick("Create workspace", "Create"),
    ]);
    let args = run_dialog(args, &mut dialog).await;
    assert_eq!(args.branch.as_deref(), Some("explicit"));
    assert_eq!(dialog.value("Workspace"), "renamed");
    assert_eq!(dialog.reviews.len(), 2);
}

#[tokio::test]
async fn cancelling_subinput_or_final_review_never_produces_creation_args() {
    for actions in [
        vec![Pick("Worktree", "Custom branch"), Cancel("Branch name")],
        vec![Tab("Worktree"), Pick("Create workspace", "Cancel")],
        vec![Cancel("Worktree")],
    ] {
        let mut dialog = Dialog::new(actions);
        let error = walkthrough(&test_paths(), "/repo".into(), named(), &mut dialog)
            .await
            .unwrap_err();
        assert!(matches!(error.downcast_ref(), Some(NavigationExit::Cancel)));
        dialog.consumed();
    }
}

#[tokio::test]
#[ignore = "manual navigation probe; requires an interactive PTY, no daemon or model calls"]
async fn interactive_navigation_walkthrough_probe() {
    let mut interaction = crate::cli::prompt::TerminalInteraction::detect(false);
    let result = walkthrough(
        &test_paths(),
        "/unreachable/coco-navigation-probe".into(),
        create_args(&["coco", "create"]),
        &mut interaction,
    )
    .await;
    assert!(!crossterm::terminal::is_raw_mode_enabled().unwrap());
    if let Ok(args) = &result {
        assert_eq!(args.name.as_deref(), Some("probe/final"));
        assert!(args.branch.is_none() && args.context.is_none() && args.send.is_none());
    }
    eprintln!("navigation-result={result:?}");
}
