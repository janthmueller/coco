use std::collections::BTreeMap;

use anyhow::Result;

use crate::cli::prompt::{
    Choice, Interaction, Navigation, NavigationOptions, ReviewField, StepSelection,
};

pub(super) struct DefaultsInteraction<'a> {
    inner: &'a mut dyn Interaction,
    remaining_defaults: bool,
    answers: AnswerMemory,
    answer_checkpoint: Option<AnswerMemory>,
    root: &'static str,
    back: bool,
    pub(super) prompts: usize,
}

#[derive(Clone, Default)]
pub(super) struct AnswerMemory {
    selections: BTreeMap<String, String>,
    texts: BTreeMap<String, String>,
}

impl<'a> DefaultsInteraction<'a> {
    pub(super) fn new(inner: &'a mut dyn Interaction) -> Self {
        Self {
            inner,
            remaining_defaults: false,
            answers: AnswerMemory::default(),
            answer_checkpoint: None,
            root: "",
            back: false,
            prompts: 0,
        }
    }

    pub(super) fn begin_step(&mut self, root: &'static str, back: bool) {
        self.root = root;
        self.back = back;
    }

    pub(super) fn stop_defaults(&mut self) {
        self.remaining_defaults = false;
    }

    pub(super) fn using_defaults(&self) -> bool {
        self.remaining_defaults
    }

    pub(super) fn snapshot_answers(&self) -> AnswerMemory {
        self.answer_checkpoint
            .as_ref()
            .unwrap_or(&self.answers)
            .clone()
    }

    pub(super) fn begin_answer_edit(&mut self) {
        // A read-only retry may keep pending input, but must not replace the
        // accepted answers that cancellation of this setting will restore.
        if self.answer_checkpoint.is_none() {
            self.answer_checkpoint = Some(self.answers.clone());
        }
    }

    pub(super) fn commit_answers(&mut self) {
        self.answer_checkpoint = None;
    }

    pub(super) fn restore_answers(&mut self, answers: AnswerMemory) {
        self.answers = answers;
        self.answer_checkpoint = None;
    }

    pub(super) fn seed_choice(&mut self, title: &str, key: &str) {
        self.answers
            .selections
            .entry(title.into())
            .or_insert_with(|| key.into());
    }

    pub(super) fn remember_choice(&mut self, title: &str, key: &str) {
        self.answers.selections.insert(title.into(), key.into());
    }

    pub(super) fn seed_text(&mut self, title: &str, value: &str) {
        self.answers
            .texts
            .entry(title.into())
            .or_insert_with(|| value.into());
    }

    fn accept<T>(&mut self, answer: Navigation<T>) -> Result<T> {
        if !matches!(answer, Navigation::Value(_)) {
            self.stop_defaults();
        }
        answer.value()
    }

    fn options(&self, title: &str, defaults: bool, selected: Option<usize>) -> NavigationOptions {
        NavigationOptions {
            allow_back: self.back || title != self.root,
            allow_defaults: defaults,
            selected,
            parent: (title != self.root && !self.root.is_empty()).then_some(self.root),
        }
    }
}

impl Interaction for DefaultsInteraction<'_> {
    fn is_interactive(&self) -> bool {
        self.inner.is_interactive()
    }

    fn select(&mut self, title: &str, choices: &[Choice]) -> Result<usize> {
        let remembered = self
            .answers
            .selections
            .get(title)
            .and_then(|key| choices.iter().position(|choice| choice.identity() == key));
        let missing = self.answers.selections.contains_key(title) && remembered.is_none();
        if missing {
            self.stop_defaults();
            self.inner
                .notice("Previous choice is no longer available. Choose again.")?;
        }
        let default = remembered.or_else(|| choices.iter().position(|choice| choice.is_default));
        if !self.remaining_defaults || default.is_none() {
            let options = self.options(title, !self.remaining_defaults, remembered);
            self.prompts += 1;
            let answer = self.inner.select_navigable(title, &[], choices, options)?;
            match self.accept(answer)? {
                StepSelection::Selected(index) => {
                    self.remember_choice(title, choices[index].identity());
                    return Ok(index);
                }
                StepSelection::UseDefaults => {
                    self.remaining_defaults = true;
                    if default.is_none() {
                        self.inner.notice(
                            "Choose a value for this step; the remaining defaults will be applied.",
                        )?;
                        return self.select(title, choices);
                    }
                }
            }
        }
        Ok(default.expect("a required choice is selected explicitly"))
    }

    fn select_with_review(
        &mut self,
        title: &str,
        fields: &[ReviewField],
        choices: &[Choice],
    ) -> Result<usize> {
        self.prompts += 1;
        let options = self.options(title, false, None);
        let answer = self
            .inner
            .select_navigable(title, fields, choices, options)?;
        match self.accept(answer)? {
            StepSelection::Selected(index) => Ok(index),
            StepSelection::UseDefaults => unreachable!("review always requires an explicit choice"),
        }
    }

    fn confirm(&mut self, title: &str) -> Result<bool> {
        self.inner.confirm(title)
    }

    fn text(&mut self, label: &str) -> Result<String> {
        self.prompts += 1;
        let initial = self
            .answers
            .texts
            .get(label)
            .map(String::as_str)
            .unwrap_or("");
        let back = self.back || label != self.root;
        let answer = self.inner.text_navigable(label, initial, back)?;
        let value = self.accept(answer)?;
        let value = if label == "Message" {
            value
        } else {
            value.trim().to_owned()
        };
        self.answers.texts.insert(label.into(), value.clone());
        Ok(value)
    }

    fn notice(&mut self, message: &str) -> Result<()> {
        self.inner.notice(message)
    }

    fn recover(
        &mut self,
        title: &str,
        error: anyhow::Error,
        allow_back: bool,
    ) -> Result<Navigation<()>> {
        self.stop_defaults();
        self.inner.recover(title, error, allow_back)
    }
}
