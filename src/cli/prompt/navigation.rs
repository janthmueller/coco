use super::*;

/// Control outcomes are distinct from discovery, validation and terminal errors.
pub(in crate::cli) enum Navigation<T> {
    Value(T),
    Back,
    Cancel,
}

#[derive(Clone, Copy, Default)]
pub(in crate::cli) struct NavigationOptions {
    pub(in crate::cli) allow_back: bool,
    pub(in crate::cli) allow_defaults: bool,
    pub(in crate::cli) selected: Option<usize>,
    pub(in crate::cli) parent: Option<&'static str>,
}

// A narrow bridge for existing Result-based command/question methods. Only the
// enclosing wizard or immediate parent handles Back; all real errors propagate.
#[derive(Debug, thiserror::Error)]
pub(in crate::cli) enum NavigationExit {
    #[error("back")]
    Back,
    #[error("workspace creation cancelled")]
    Cancel,
}

impl<T> Navigation<T> {
    pub(in crate::cli) fn value(self) -> Result<T> {
        match self {
            Self::Value(value) => Ok(value),
            Self::Back => Err(NavigationExit::Back.into()),
            Self::Cancel => Err(NavigationExit::Cancel.into()),
        }
    }
}

pub(in crate::cli) fn parent_choice<T>(result: Result<T>) -> Result<Option<T>> {
    match result {
        Err(error) if matches!(error.downcast_ref(), Some(NavigationExit::Back)) => Ok(None),
        other => other.map(Some),
    }
}

pub(super) fn run(
    title: &str,
    fields: &[ReviewField],
    choices: &[Choice],
    options: NavigationOptions,
) -> Result<Navigation<StepSelection>> {
    let mut terminal = PickerTerminal::enter(io::stderr().lock())?;
    terminal.enable_paste()?;
    let mut state = PickerState::from_choices(choices);
    if let Some(selected) = options.selected.filter(|index| *index < choices.len()) {
        state.selected = selected;
    }
    let palette = Palette::stderr();
    let title = options.parent.map_or_else(
        || title.to_owned(),
        |parent| {
            let child = match title {
                "Code from branch" | "Use branch" => "Branch",
                "Code from workspace" | "Context from workspace" => "Workspace",
                "Context size" => "Size",
                other => other,
            };
            format!("{parent} > {child}")
        },
    );
    let outcome = loop {
        let (columns, rows) = size().unwrap_or((100, 24));
        let layout = PickerLayout::with_review(columns, rows.saturating_sub(1), fields.len())?;
        state.visible_rows = layout.visible_rows;
        let mut lines =
            picker_lines_with_review(&title, fields, choices, &state, layout.width, palette);
        lines.push(palette.paint(Tone::Dim, truncate_line(hint(options), layout.width)));
        terminal.draw(&lines, rows)?;
        if let Event::Key(key) = event::read().context("could not read terminal input")?
            && matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        {
            if let Some(action) = navigate_key(key, options.allow_back) {
                break action;
            }
            match state.handle_step_key(key, choices.len(), options.allow_defaults) {
                PickerAction::Selected(index) => {
                    break Navigation::Value(StepSelection::Selected(index));
                }
                PickerAction::UseDefaults => break Navigation::Value(StepSelection::UseDefaults),
                PickerAction::Cancelled | PickerAction::Interrupted => break Navigation::Cancel,
                PickerAction::Continue => {}
            }
        }
    };
    terminal
        .finish()
        .context("could not restore the terminal")?;
    Ok(outcome)
}

pub(super) fn navigate_key<T>(key: KeyEvent, allow_back: bool) -> Option<Navigation<T>> {
    match key.code {
        KeyCode::Esc => Some(if allow_back {
            Navigation::Back
        } else {
            Navigation::Cancel
        }),
        KeyCode::BackTab if allow_back => Some(Navigation::Back),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Navigation::Cancel)
        }
        _ => None,
    }
}

fn hint(options: NavigationOptions) -> &'static str {
    match (options.allow_back, options.allow_defaults) {
        (true, true) => "Esc back · Tab defaults",
        (false, true) => "Esc cancel · Tab defaults",
        (true, false) => "Esc back",
        (false, false) => "Esc cancel",
    }
}
