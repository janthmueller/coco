use std::collections::BTreeSet;

use crate::cli::prompt::{Navigation, NavigationExit};

use super::*;

mod draft;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Step {
    Name,
    Worktree,
    Code,
    Changes,
    Context,
    Profile,
    Model,
    Cleanup,
    After,
    Review,
}

const STEPS: [Step; 10] = [
    Step::Name,
    Step::Worktree,
    Step::Code,
    Step::Changes,
    Step::Context,
    Step::Profile,
    Step::Model,
    Step::Cleanup,
    Step::After,
    Step::Review,
];

impl Step {
    fn title(self) -> &'static str {
        match self {
            Self::Name => "Workspace name",
            Self::Worktree => "Worktree",
            Self::Code => "Code",
            Self::Changes => "Git changes",
            Self::Context => "Context",
            Self::Profile => "Profile",
            Self::Model => "Model",
            Self::Cleanup => "Cleanup",
            Self::After => "After create",
            Self::Review => "Create workspace",
        }
    }

    fn next(self) -> Self {
        STEPS[STEPS.iter().position(|step| *step == self).unwrap() + 1]
    }

    fn applicable(self, args: &CreateArgs) -> bool {
        self != Self::Code || args.checkout.is_none()
    }

    fn previous(history: &mut Vec<Self>, args: &CreateArgs) -> Option<Self> {
        while let Some(step) = history.pop() {
            if step.applicable(args) {
                return Some(step);
            }
        }
        None
    }
}

impl CreateWalkthrough<'_> {
    pub(super) async fn run(mut self) -> Result<CreateArgs> {
        let mut current = Step::Name;
        let mut history = Vec::new();
        let mut completed = BTreeSet::new();
        let mut reopen = false;
        let mut editing = false;
        loop {
            let can_go_back = editing
                || history
                    .iter()
                    .any(|step: &Step| step.applicable(&self.args));
            self.interaction.begin_step(current.title(), can_go_back);
            if reopen || current == Step::Review {
                self.interaction.begin_answer_edit();
            }
            let before = draft::Snapshot::capture(&self);
            let prompts = self.interaction.prompts;
            let result = self
                .execute_step(current, reopen, completed.contains(&current))
                .await;
            let result = match result {
                Err(error) if error.downcast_ref::<NavigationExit>().is_none() => {
                    let recovery = self
                        .interaction
                        .recover(current.title(), error, can_go_back)?;
                    match recovery {
                        Navigation::Value(()) => {
                            before.restore_draft(&mut self);
                            continue;
                        }
                        Navigation::Back => Err(NavigationExit::Back.into()),
                        Navigation::Cancel => Err(NavigationExit::Cancel.into()),
                    }
                }
                other => other,
            };
            let action = match result {
                Ok(action) => action,
                Err(error) if matches!(error.downcast_ref(), Some(NavigationExit::Back)) => {
                    if reopen || current == Step::Review {
                        before.restore(&mut self);
                    } else {
                        before.restore_draft(&mut self);
                    }
                    self.interaction.stop_defaults();
                    if editing {
                        current = self.edit_settings()?.unwrap_or(Step::Review);
                        reopen = current != Step::Review;
                        editing = reopen;
                    } else if let Some(previous) = Step::previous(&mut history, &self.args) {
                        current = previous;
                        reopen = true;
                    } else {
                        return Err(NavigationExit::Cancel.into());
                    }
                    continue;
                }
                Err(error) => return Err(error),
            };
            self.interaction.commit_answers();
            if current == Step::Review {
                match action {
                    0 => return Ok(self.args),
                    1 => {
                        self.interaction.stop_defaults();
                        if let Some(step) = self.edit_settings()? {
                            current = step;
                            reopen = true;
                            editing = true;
                        }
                    }
                    _ => return Err(NavigationExit::Cancel.into()),
                }
                continue;
            }
            self.invalidate_dependents(current, &before.args, &mut completed);
            completed.insert(current);
            if self.interaction.prompts > prompts && history.last() != Some(&current) {
                history.push(current);
            }
            current = if editing {
                Step::Review
            } else {
                current.next()
            };
            reopen = false;
            editing = false;
        }
    }

    async fn execute_step(&mut self, step: Step, reopened: bool, complete: bool) -> Result<usize> {
        if reopened {
            self.reopen_step(step).await?;
        }
        if step == Step::Review {
            return self.review_draft().await;
        }
        if reopened || !complete {
            self.ask_step(step, reopened).await?;
        }
        Ok(0)
    }

    async fn review_draft(&mut self) -> Result<usize> {
        self.prepare_git_review().await?;
        let carries = self.args.dirty || self.args.carry_changes;
        if carries && self.source.base_sha != self.source.head_sha {
            self.interaction.stop_defaults();
            self.interaction.notice(
                "Copying local changes needs the current checkout's commit. Change Code or Git changes before creating.",
            )?;
            return Ok(1); // Deliberate correction, not implicit loss of carry intent.
        }
        self.confirm_creation()
    }

    async fn ask_step(&mut self, step: Step, reopened: bool) -> Result<()> {
        match step {
            Step::Name => self.prompt_name(),
            Step::Worktree => self.prompt_worktree().await,
            Step::Code => self.prompt_code_base().await,
            Step::Changes if reopened => self.prompt_copy_policy(true),
            Step::Changes => self.prompt_local_changes(),
            Step::Context => self.prompt_context().await,
            Step::Profile => self.prompt_profile(),
            Step::Model => self.prompt_model().await,
            Step::Cleanup => self.prompt_temporary(),
            Step::After => self.prompt_post_action(),
            Step::Review => unreachable!("review is handled separately"),
        }
    }

    fn edit_settings(&mut self) -> Result<Option<Step>> {
        self.interaction.begin_step("Edit settings", true);
        let steps: Vec<_> = STEPS
            .into_iter()
            .filter(|step| *step != Step::Review && step.applicable(&self.args))
            .collect();
        let fields = self.review_fields();
        let choices = steps
            .iter()
            .map(|step| {
                let label = if *step == Step::Name {
                    "Workspace"
                } else {
                    step.title()
                };
                let detail = fields
                    .iter()
                    .find(|field| field.label == label)
                    .map(|field| field.value.clone())
                    .or_else(|| (*step == Step::Cleanup).then(|| "keep workspace".into()));
                Choice::new(label, detail)
            })
            .collect::<Vec<_>>();
        parent_choice(
            self.interaction
                .select_with_review("Edit settings", &[], &choices),
        )
        .map(|selection| selection.map(|index| steps[index]))
    }

    fn invalidate_dependents(
        &mut self,
        step: Step,
        previous: &CreateArgs,
        completed: &mut BTreeSet<Step>,
    ) {
        if step == Step::Worktree && previous.checkout != self.args.checkout {
            completed.remove(&Step::Code);
        }
        if step == Step::Code || step == Step::Worktree {
            self.reviewed_code = None;
            self.args.expected_base_sha = None;
        }
    }
}
