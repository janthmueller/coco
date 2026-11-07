use super::*;

impl CreateWalkthrough<'_> {
    pub(super) fn prompt_temporary(&mut self) -> Result<()> {
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
            self.args.keep_thread = self.interaction.select("Conversation", &choices)? == 1;
        }
        Ok(())
    }
}
