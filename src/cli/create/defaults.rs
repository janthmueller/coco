use anyhow::Result;

use crate::cli::prompt::{Choice, Interaction, ReviewField, StepSelection};

pub(super) struct DefaultsInteraction<'a> {
    inner: &'a mut dyn Interaction,
    remaining_defaults: bool,
}

impl<'a> DefaultsInteraction<'a> {
    pub(super) fn new(inner: &'a mut dyn Interaction) -> Self {
        Self {
            inner,
            remaining_defaults: false,
        }
    }

    pub(super) fn using_defaults(&self) -> bool {
        self.remaining_defaults
    }
}

impl Interaction for DefaultsInteraction<'_> {
    fn is_interactive(&self) -> bool {
        self.inner.is_interactive()
    }

    fn select(&mut self, title: &str, choices: &[Choice]) -> Result<usize> {
        let default = choices.iter().position(|choice| choice.is_default);
        if !self.remaining_defaults {
            match self.inner.select_step(title, choices)? {
                StepSelection::Selected(index) => return Ok(index),
                StepSelection::UseDefaults => {
                    self.remaining_defaults = true;
                    if default.is_none() {
                        self.inner.notice(
                            "Choose a value for this step; the remaining defaults will be applied.",
                        )?;
                    }
                }
            }
        }
        match default {
            Some(index) => Ok(index),
            None => self.inner.select(title, choices),
        }
    }

    fn select_with_review(
        &mut self,
        title: &str,
        fields: &[ReviewField],
        choices: &[Choice],
    ) -> Result<usize> {
        self.inner.select_with_review(title, fields, choices)
    }

    fn confirm(&mut self, title: &str) -> Result<bool> {
        self.inner.confirm(title)
    }

    fn text(&mut self, label: &str) -> Result<String> {
        self.inner.text(label)
    }

    fn notice(&mut self, message: &str) -> Result<()> {
        self.inner.notice(message)
    }
}
