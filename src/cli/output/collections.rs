use std::io::{self, IsTerminal};

use crossterm::terminal;

use crate::domain::CodexModel;
use crate::protocol::{RepositorySummary, WorkspaceListItem};

use super::super::style::{Palette, Tone};
use super::{phase_presentation, safe_line};

const MAX_TABLE_WIDTH: usize = 160;

pub(in crate::cli) fn print_workspace_list(
    workspaces: &[WorkspaceListItem],
    include_repository: bool,
    include_resources: bool,
) {
    print!(
        "{}",
        render_workspace_list_for_stdout(workspaces, include_repository, include_resources)
    );
}

pub(in crate::cli) fn render_workspace_list_for_stdout(
    workspaces: &[WorkspaceListItem],
    include_repository: bool,
    include_resources: bool,
) -> String {
    render_workspace_list(
        workspaces,
        include_repository,
        include_resources,
        stdout_width(),
        Palette::stdout(),
    )
}

pub(in crate::cli) fn print_repository_list(repositories: &[RepositorySummary]) {
    print!(
        "{}",
        render_repository_list(repositories, stdout_width(), Palette::stdout())
    );
}

pub(in crate::cli) fn print_model_list(models: &[CodexModel]) {
    print!(
        "{}",
        render_model_list(models, stdout_width(), Palette::stdout())
    );
}

fn render_workspace_list(
    workspaces: &[WorkspaceListItem],
    include_repository: bool,
    include_resources: bool,
    width: usize,
    palette: Palette,
) -> String {
    if workspaces.is_empty() {
        return format!("{}\n", palette.paint(Tone::Dim, "No workspaces."));
    }
    let rows = workspaces
        .iter()
        .map(|item| {
            let mut cells = Vec::with_capacity(
                3 + usize::from(include_repository) + 3 * usize::from(include_resources),
            );
            if include_repository {
                cells.push(Cell::new(
                    item.repository.root_path.display().to_string(),
                    Tone::Dim,
                ));
            }
            let phase = phase_presentation(item.workspace.phase);
            cells.push(Cell::new(item.workspace.name.clone(), Tone::Primary));
            cells.push(Cell::new(
                format!("{} {}", phase.marker, phase.label),
                phase.tone,
            ));
            if include_resources {
                let (rss, processes, cpu) = resource_cells(item.runtime_resources.as_ref());
                cells.push(Cell::new(rss, Tone::Dim));
                cells.push(Cell::new(processes, Tone::Dim));
                cells.push(Cell::new(cpu, Tone::Dim));
            }
            cells.push(Cell::new(
                item.workspace
                    .branch_name
                    .clone()
                    .unwrap_or_else(|| "detached".to_owned()),
                Tone::Dim,
            ));
            cells
        })
        .collect::<Vec<_>>();
    if include_repository && include_resources {
        render_table(
            &[
                "REPOSITORY",
                "WORKSPACE",
                "STATE",
                "MEMORY",
                "PROCS",
                "CPU",
                "BRANCH",
            ],
            &rows,
            &[36, 32, 26, 14, 8, 10, 48],
            width,
            palette,
        )
    } else if include_repository {
        render_table(
            &["REPOSITORY", "WORKSPACE", "STATE", "BRANCH"],
            &rows,
            &[36, 32, 26, 48],
            width,
            palette,
        )
    } else if include_resources {
        render_table(
            &["WORKSPACE", "STATE", "MEMORY", "PROCS", "CPU", "BRANCH"],
            &rows,
            &[32, 26, 14, 8, 10, 48],
            width,
            palette,
        )
    } else {
        render_table(
            &["WORKSPACE", "STATE", "BRANCH"],
            &rows,
            &[32, 26, 48],
            width,
            palette,
        )
    }
}

fn resource_cells(
    resources: Option<&crate::domain::runtime::WorkspaceRuntimeResources>,
) -> (String, String, String) {
    let Some(resources) = resources else {
        return ("—".to_owned(), "—".to_owned(), "—".to_owned());
    };
    if resources.state != crate::domain::runtime::WorkspaceRuntimeState::Running {
        return ("—".to_owned(), "—".to_owned(), "—".to_owned());
    }
    (
        resources
            .memory_current_bytes
            .or(resources.resident_memory_bytes)
            .map_or_else(|| "—".to_owned(), super::format_bytes),
        resources
            .process_count
            .map_or_else(|| "—".to_owned(), |count| count.to_string()),
        resources
            .cpu_percent
            .map_or_else(|| "—".to_owned(), |cpu| format!("{cpu:.1}%")),
    )
}

fn render_repository_list(
    repositories: &[RepositorySummary],
    width: usize,
    palette: Palette,
) -> String {
    if repositories.is_empty() {
        return format!("{}\n", palette.paint(Tone::Dim, "No repositories."));
    }
    let rows = repositories
        .iter()
        .map(|repository| {
            vec![
                Cell::new(repository.display_name.clone(), Tone::Primary),
                Cell::new(repository.root_path.display().to_string(), Tone::Dim),
            ]
        })
        .collect::<Vec<_>>();
    render_table(&["REPOSITORY", "PATH"], &rows, &[32, 100], width, palette)
}

fn render_model_list(models: &[CodexModel], width: usize, palette: Palette) -> String {
    if models.is_empty() {
        return format!("{}\n", palette.paint(Tone::Dim, "No models available."));
    }
    let rows = models
        .iter()
        .map(|model| {
            let model_name = if model.is_default {
                format!("{} (default)", model.model)
            } else {
                model.model.clone()
            };
            let reasoning = model
                .supported_reasoning_efforts
                .iter()
                .map(|effort| effort.reasoning_effort.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            vec![
                Cell::new(
                    model_name,
                    if model.is_default {
                        Tone::Green
                    } else {
                        Tone::Primary
                    },
                ),
                Cell::new(model.display_name.clone(), Tone::Primary),
                Cell::new(
                    if reasoning.is_empty() {
                        "-".to_owned()
                    } else {
                        reasoning
                    },
                    Tone::Dim,
                ),
            ]
        })
        .collect::<Vec<_>>();
    render_table(
        &["MODEL", "NAME", "REASONING"],
        &rows,
        &[38, 32, 48],
        width,
        palette,
    )
}

#[derive(Debug, Clone)]
pub(super) struct Cell {
    text: String,
    tone: Tone,
}

impl Cell {
    pub(super) fn new(text: String, tone: Tone) -> Self {
        Self {
            text: safe_line(&text),
            tone,
        }
    }
}

pub(super) fn render_table(
    headers: &[&str],
    rows: &[Vec<Cell>],
    caps: &[usize],
    max_width: usize,
    palette: Palette,
) -> String {
    debug_assert_eq!(headers.len(), caps.len());
    debug_assert!(rows.iter().all(|row| row.len() == headers.len()));
    let mut widths = headers
        .iter()
        .enumerate()
        .map(|(index, header)| {
            rows.iter()
                .map(|row| display_width(&row[index].text))
                .chain([display_width(header)])
                .max()
                .unwrap_or_default()
                .min(caps[index])
        })
        .collect::<Vec<_>>();
    fit_widths(&mut widths, headers, max_width);

    let mut output = String::new();
    let header = headers
        .iter()
        .enumerate()
        .map(|(index, header)| padded(header, widths[index], index + 1 == headers.len()))
        .collect::<Vec<_>>()
        .join("  ");
    output.push_str(&palette.paint(Tone::Bold, header));
    output.push('\n');
    for row in rows {
        let line = row
            .iter()
            .enumerate()
            .map(|(index, cell)| {
                palette.paint(
                    cell.tone,
                    padded(&cell.text, widths[index], index + 1 == row.len()),
                )
            })
            .collect::<Vec<_>>()
            .join("  ");
        output.push_str(&line);
        output.push('\n');
    }
    output
}

fn fit_widths(widths: &mut [usize], headers: &[&str], max_width: usize) {
    if max_width == usize::MAX {
        return;
    }
    let separators = widths.len().saturating_sub(1) * 2;
    while widths.iter().sum::<usize>() + separators > max_width {
        let candidate = widths
            .iter()
            .enumerate()
            .filter(|(index, width)| **width > display_width(headers[*index]).min(3))
            .max_by_key(|(_, width)| **width)
            .map(|(index, _)| index);
        let Some(index) = candidate else {
            break;
        };
        widths[index] -= 1;
    }
}

fn padded(value: &str, width: usize, last: bool) -> String {
    let value = truncate(value, width);
    if last {
        value
    } else {
        format!("{value:<width$}")
    }
}

fn truncate(value: &str, width: usize) -> String {
    if display_width(value) <= width {
        return value.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut truncated = value
        .chars()
        .take(width.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}

fn display_width(value: &str) -> usize {
    value.chars().count()
}

pub(super) fn stdout_width() -> usize {
    if io::stdout().is_terminal() {
        usize::from(terminal::size().map_or(120, |(width, _)| width))
            .saturating_sub(1)
            .min(MAX_TABLE_WIDTH)
    } else {
        usize::MAX
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::runtime::{
        WorkspaceResourceScope, WorkspaceRuntimeBackend, WorkspaceRuntimeResources,
        WorkspaceRuntimeState,
    };

    #[test]
    fn table_alignment_is_plain_and_bounded_before_styling() {
        let rows = vec![
            vec![
                Cell::new("feat/login".to_owned(), Tone::Primary),
                Cell::new("● Ready".to_owned(), Tone::Green),
            ],
            vec![
                Cell::new("a-very-long-workspace".to_owned(), Tone::Primary),
                Cell::new("● Working".to_owned(), Tone::Cyan),
            ],
        ];
        let rendered = render_table(
            &["WORKSPACE", "STATE"],
            &rows,
            &[12, 12],
            23,
            Palette::plain(),
        );
        assert_eq!(
            rendered,
            "WORKSPACE     STATE\nfeat/login    ● Ready\na-very-long…  ● Working\n"
        );
        assert!(!rendered.contains("\u{1b}["));
    }

    #[test]
    fn colored_tables_style_without_changing_visible_content() {
        let rows = vec![vec![Cell::new("● Ready".to_owned(), Tone::Green)]];
        let colored = render_table(&["STATE"], &rows, &[20], 20, Palette::colored());
        assert!(colored.contains("\u{1b}["));
        assert!(colored.contains("● Ready"));
    }

    #[test]
    fn resource_cells_show_only_scan_friendly_measurements() {
        let resources = WorkspaceRuntimeResources {
            backend: WorkspaceRuntimeBackend::ExecServer,
            state: WorkspaceRuntimeState::Running,
            scope: WorkspaceResourceScope::ProcessTree,
            process_id: Some(42),
            process_count: Some(3),
            task_count: None,
            resident_memory_bytes: Some(25 * 1024 * 1024),
            memory_current_bytes: None,
            cpu_percent: Some(12.34),
            cpu_usage_usec: None,
            cgroup_unit: None,
            events: None,
            sampled_at_ms: Some(1),
        };
        assert_eq!(
            resource_cells(Some(&resources)),
            ("25.0 MiB".to_owned(), "3".to_owned(), "12.3%".to_owned())
        );
        assert_eq!(
            resource_cells(None),
            ("—".to_owned(), "—".to_owned(), "—".to_owned())
        );
    }
}
