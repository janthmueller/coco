use super::*;

pub(super) struct Snapshot {
    pub(super) args: CreateArgs,
    code: Option<String>,
    context: Option<String>,
    previous_context: Option<(String, String)>,
    changes_preset: bool,
    changes_prompted: Option<crate::protocol::GitChangeSummary>,
    answers: defaults::AnswerMemory,
}

impl Snapshot {
    pub(super) fn capture(wizard: &CreateWalkthrough<'_>) -> Self {
        Self {
            args: wizard.args.clone(),
            code: wizard.selected_code_workspace.clone(),
            context: wizard.selected_context_source.clone(),
            previous_context: wizard.previous_context.clone(),
            changes_preset: wizard.changes_preset,
            changes_prompted: wizard.changes_prompted.clone(),
            answers: wizard.interaction.snapshot_answers(),
        }
    }

    pub(super) fn restore(self, wizard: &mut CreateWalkthrough<'_>) {
        self.restore_inner(wizard, true);
    }

    // Retry/first-time Back keeps pending UI input, but never commits it to args.
    pub(super) fn restore_draft(self, wizard: &mut CreateWalkthrough<'_>) {
        self.restore_inner(wizard, false);
    }

    fn restore_inner(self, wizard: &mut CreateWalkthrough<'_>, restore_answers: bool) {
        wizard.args = self.args;
        wizard.selected_code_workspace = self.code;
        wizard.selected_context_source = self.context;
        wizard.previous_context = self.previous_context;
        wizard.changes_preset = self.changes_preset;
        wizard.changes_prompted = self.changes_prompted;
        wizard.reviewed_code = None;
        if restore_answers {
            wizard.interaction.restore_answers(self.answers);
        }
    }
}

impl CreateWalkthrough<'_> {
    pub(super) async fn reopen_step(&mut self, step: Step) -> Result<()> {
        self.interaction.stop_defaults();
        match step {
            Step::Name => {
                if let Some(name) = self.args.name.take() {
                    self.interaction.seed_text("Workspace name", &name);
                }
            }
            Step::Worktree => self.reopen_worktree(),
            Step::Code => {
                self.refresh_checkout().await?;
                self.reopen_code();
            }
            Step::Changes => {
                self.seed_changes();
                self.refresh_checkout().await?;
                self.changes_preset = false;
            }
            Step::Context => self.reopen_context(),
            Step::Profile => {
                if let Some(profile) = self.args.profile.take() {
                    self.interaction.seed_choice("Profile", "Choose profile");
                    self.interaction.seed_text("Profile name", &profile);
                }
            }
            Step::Model => {
                if let Some(model) = self.args.model.take() {
                    self.interaction.seed_choice("Model", "Choose model");
                    self.interaction.seed_choice("Choose model", &model);
                }
            }
            Step::Cleanup => {
                if self.args.temporary {
                    self.interaction
                        .seed_choice("Cleanup", "Clean up after a TUI session");
                }
                if self.args.keep_thread {
                    self.interaction
                        .seed_choice("Conversation", "Keep conversation");
                }
                self.args.temporary = false;
                self.args.keep_thread = false;
            }
            Step::After => {
                let label = match (self.args.send.is_some(), self.args.jump) {
                    (false, false) => "Return to shell",
                    (true, false) => "Send message",
                    (false, true) => "Open Codex",
                    (true, true) => "Send, then open Codex",
                };
                self.interaction.seed_choice("After create", label);
                if let Some(message) = self.args.send.take() {
                    self.interaction.seed_text("Message", &message);
                }
                self.args.jump = false;
            }
            Step::Review => {}
        }
        Ok(())
    }

    async fn refresh_checkout(&mut self) -> Result<()> {
        let state = self
            .inspector
            .inspect(crate::protocol::RepositoryGitInspectParams {
                path: self.repository_path.clone(),
                base: None,
                include_branches: false,
            })
            .await?;
        if state.repository_id != self.source.repository_id {
            bail!("the selected repository changed; run create again");
        }
        self.source = state;
        Ok(())
    }

    fn split_legacy_context(&mut self) {
        if let Some(source) = self.args.fork_from.take() {
            self.args.base_workspace = Some(source.clone());
            self.args.context = Some(format!("workspace:{source}"));
        }
    }

    fn reopen_worktree(&mut self) {
        if let Some(branch) = self.args.branch.take() {
            self.interaction.seed_choice("Worktree", "Custom branch");
            self.interaction.seed_text("Branch name", &branch);
        }
        if let Some(branch) = self.args.checkout.take() {
            self.interaction.seed_choice("Worktree", "Existing branch");
            self.interaction
                .seed_choice("Use branch", &format!("refs/heads/{branch}"));
        }
        if self.args.detached {
            self.interaction.seed_choice("Worktree", "Detached HEAD");
        }
        self.args.detached = false;
    }

    fn reopen_code(&mut self) {
        self.split_legacy_context();
        if let Some(base) = self.args.base.take() {
            if base.starts_with("refs/heads/") || base.starts_with("refs/remotes/") {
                self.interaction.seed_choice("Code", "Branch");
                self.interaction.seed_choice("Code from branch", &base);
            } else {
                self.interaction.seed_choice("Code", "Other revision");
                self.interaction.seed_text("Git revision", &base);
            }
        }
        if let Some(workspace) = self.args.base_workspace.take() {
            self.interaction.seed_choice("Code", "Workspace");
            self.interaction
                .seed_choice("Code from workspace", &workspace);
        }
        self.selected_code_workspace = None;
    }

    fn reopen_context(&mut self) {
        self.split_legacy_context();
        self.previous_context = None;
        if let Some(reference) = self.args.context.take() {
            let label = self
                .selected_context_source
                .clone()
                .unwrap_or_else(|| context_reference_label(&reference));
            self.previous_context = Some((reference.clone(), label));
            self.interaction
                .seed_choice("Context", &format!("selected:{reference}"));
            if let Some(id) = reference.strip_prefix("workspace:") {
                self.interaction.seed_choice("Context from workspace", id);
            } else if let Some(id) = reference.strip_prefix("thread:") {
                self.interaction.seed_text("Codex thread ID", id);
            }
            self.interaction.seed_choice(
                "Context size",
                if self.args.compact_context {
                    "Compact context"
                } else {
                    "Full context"
                },
            );
        }
        self.args.compact_context = false;
        self.selected_context_source = None;
    }

    fn seed_changes(&mut self) {
        let key = if self.args.dirty || self.args.carry_untracked {
            "tracked+untracked"
        } else if self.args.carry_changes {
            "tracked"
        } else {
            "Don't copy"
        };
        self.interaction.seed_choice("Git changes", key);
    }
}
