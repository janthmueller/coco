use super::*;

impl CreateWalkthrough<'_> {
    pub(super) fn prompt_temporary(&mut self) -> Result<()> {
        loop {
            if !self.args.temporary {
                let choices = [
                    Choice::new("Keep workspace", None).with_default_marker(),
                    Choice::new(
                        "Clean up after a TUI session",
                        Some("only when safe".to_owned()),
                    ),
                ];
                self.args.temporary = self.interaction.select("Cleanup", &choices)? == 1;
            }
            if self.args.temporary && !self.args.keep_thread {
                let choices = [
                    Choice::new("Delete with workspace", None).with_default_marker(),
                    Choice::new("Keep conversation", None),
                ];
                let Some(selected) =
                    parent_choice(self.interaction.select("Conversation", &choices))?
                else {
                    self.args.temporary = false;
                    continue;
                };
                self.args.keep_thread = selected == 1;
            }
            return Ok(());
        }
    }
}
