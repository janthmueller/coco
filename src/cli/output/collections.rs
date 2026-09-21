use std::collections::HashMap;
use std::io::{self, IsTerminal};

use crossterm::terminal;

use crate::domain::CodexModel;
use crate::protocol::{RepositorySummary, WorkspaceListItem, WorkspaceUsageItem};

use super::super::style::{Palette, Tone};
use super::usage::usage_cells;
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
        None,
        stdout_width(),
        Palette::stdout(),
    )
}

pub(in crate::cli) fn print_workspace_status_list(
    workspaces: &[WorkspaceListItem],
    include_repository: bool,
    include_resources: bool,
    usage: Option<&[WorkspaceUsageItem]>,
    tree: bool,
) {
    print!(
        "{}",
        render_workspace_status_list_for_stdout(
            workspaces,
            include_repository,
            include_resources,
            usage,
            tree,
        )
    );
}

pub(in crate::cli) fn render_workspace_status_list_for_stdout(
    workspaces: &[WorkspaceListItem],
    include_repository: bool,
    include_resources: bool,
    usage: Option<&[WorkspaceUsageItem]>,
    tree: bool,
) -> String {
    let width = stdout_width();
    let palette = Palette::stdout();
    if tree {
        render_workspace_tree(
            workspaces,
            include_repository,
            include_resources,
            usage,
            width,
            palette,
        )
    } else {
        render_workspace_list(
            workspaces,
            include_repository,
            include_resources,
            usage,
            width,
            palette,
        )
    }
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
    usage: Option<&[WorkspaceUsageItem]>,
    width: usize,
    palette: Palette,
) -> String {
    if workspaces.is_empty() {
        return format!("{}\n", palette.paint(Tone::Dim, "No workspaces."));
    }
    let usage_by_workspace = usage_index(usage);
    let include_usage = usage_by_workspace.is_some();
    let include_activity = workspaces.iter().any(|item| item.activity.is_some());
    let rows = workspaces
        .iter()
        .map(|item| {
            workspace_row(
                item,
                Cell::new(item.workspace.name.clone(), Tone::Primary),
                include_repository,
                include_resources,
                usage_by_workspace.as_ref(),
            )
        })
        .collect::<Vec<_>>();
    let (headers, caps) = workspace_table_shape(
        include_repository,
        include_resources,
        include_usage,
        include_activity,
        false,
    );
    render_table(&headers, &rows, &caps, width, palette)
}

fn render_workspace_tree(
    workspaces: &[WorkspaceListItem],
    include_repository: bool,
    include_resources: bool,
    usage: Option<&[WorkspaceUsageItem]>,
    width: usize,
    palette: Palette,
) -> String {
    if workspaces.is_empty() {
        return format!("{}\n", palette.paint(Tone::Dim, "No workspaces."));
    }
    let usage_by_workspace = usage_index(usage);
    let include_usage = usage_by_workspace.is_some();
    let include_activity = workspaces.iter().any(|item| item.activity.is_some());
    let (headers, caps) = workspace_table_shape(
        false,
        include_resources,
        include_usage,
        include_activity,
        true,
    );

    if include_repository {
        let groups = repository_groups(workspaces);
        let mut output = String::new();
        for (index, group) in groups.iter().enumerate() {
            if index > 0 {
                output.push('\n');
            }
            let repository = &group[0].repository;
            let heading = safe_line(&repository.root_path.display().to_string());
            output.push_str(&palette.paint(Tone::Bold, truncate(&heading, width)));
            output.push('\n');
            let roots = workspace_tree(group);
            let mut rows = Vec::new();
            append_tree_rows(
                &roots,
                &mut Vec::new(),
                &mut rows,
                include_resources,
                usage_by_workspace.as_ref(),
                headers.len(),
            );
            output.push_str(&render_table(&headers, &rows, &caps, width, palette));
        }
        return output;
    }

    let roots = workspace_tree(workspaces);
    let mut rows = Vec::new();
    append_tree_rows(
        &roots,
        &mut Vec::new(),
        &mut rows,
        include_resources,
        usage_by_workspace.as_ref(),
        headers.len(),
    );
    render_table(&headers, &rows, &caps, width, palette)
}

type UsageIndex<'a> = HashMap<&'a str, &'a WorkspaceUsageItem>;

fn usage_index(usage: Option<&[WorkspaceUsageItem]>) -> Option<UsageIndex<'_>> {
    usage.map(|usage| {
        usage
            .iter()
            .map(|item| (item.workspace.id.as_str(), item))
            .collect()
    })
}

fn workspace_row(
    item: &WorkspaceListItem,
    workspace_cell: Cell,
    include_repository: bool,
    include_resources: bool,
    usage_by_workspace: Option<&UsageIndex<'_>>,
) -> Vec<Cell> {
    let include_usage = usage_by_workspace.is_some();
    let mut cells = Vec::with_capacity(
        3 + usize::from(include_repository)
            + 3 * usize::from(include_resources)
            + 3 * usize::from(include_usage),
    );
    if include_repository {
        cells.push(Cell::new(
            item.repository.root_path.display().to_string(),
            Tone::Dim,
        ));
    }
    cells.push(workspace_cell);
    cells.push(workspace_state_cell(item));
    if include_resources {
        let (rss, processes, cpu) = resource_cells(item.runtime_resources.as_ref());
        cells.push(Cell::new(rss, Tone::Dim));
        cells.push(Cell::new(processes, Tone::Dim));
        cells.push(Cell::new(cpu, Tone::Dim));
    }
    if include_usage {
        let usage =
            usage_by_workspace.and_then(|usage| usage.get(item.workspace.id.as_str()).copied());
        let (tokens, context, cost) = usage_cells(usage);
        cells.push(Cell::new(tokens, Tone::Primary));
        cells.push(Cell::new(context, Tone::Dim));
        cells.push(Cell::new(cost, Tone::Dim));
    }
    cells.push(Cell::new(
        item.workspace
            .branch_name
            .clone()
            .unwrap_or_else(|| "detached".to_owned()),
        Tone::Dim,
    ));
    cells
}

fn workspace_table_shape(
    include_repository: bool,
    include_resources: bool,
    include_usage: bool,
    include_activity: bool,
    tree: bool,
) -> (Vec<&'static str>, Vec<usize>) {
    let mut headers = Vec::with_capacity(
        3 + usize::from(include_repository)
            + 3 * usize::from(include_resources)
            + 3 * usize::from(include_usage),
    );
    let mut caps = Vec::with_capacity(headers.capacity());
    if include_repository {
        headers.push("REPOSITORY");
        caps.push(36);
    }
    headers.extend(["WORKSPACE", "STATE"]);
    caps.extend([
        if tree { 64 } else { 32 },
        if include_activity { 48 } else { 18 },
    ]);
    if include_resources {
        headers.extend(["MEMORY", "PROCS", "CPU"]);
        caps.extend([14, 8, 10]);
    }
    if include_usage {
        headers.extend(["TOKENS", "CONTEXT", "COST"]);
        caps.extend([24, 16, 18]);
    }
    headers.push("BRANCH");
    caps.push(48);
    (headers, caps)
}

fn workspace_state_cell(item: &WorkspaceListItem) -> Cell {
    let phase = phase_presentation(item.workspace.phase);
    let state = format!("{} {}", phase.marker, phase.label);
    match item.activity.as_ref() {
        Some(activity) => Cell::segmented(
            [
                (state, phase.tone),
                (" · ".to_owned(), Tone::Dim),
                (activity.label.clone(), Tone::Primary),
            ],
            phase.tone,
        ),
        None => Cell::new(state, phase.tone),
    }
}

#[derive(Debug)]
struct WorkspaceTreeNode<'a> {
    component: &'a str,
    workspace: Option<&'a WorkspaceListItem>,
    children: Vec<Self>,
}

fn workspace_tree(workspaces: &[WorkspaceListItem]) -> Vec<WorkspaceTreeNode<'_>> {
    let mut roots = Vec::new();
    for workspace in workspaces {
        let components = workspace.workspace.name.split('/').collect::<Vec<_>>();
        insert_tree_node(&mut roots, &components, workspace);
    }
    roots
}

fn insert_tree_node<'a>(
    nodes: &mut Vec<WorkspaceTreeNode<'a>>,
    components: &[&'a str],
    workspace: &'a WorkspaceListItem,
) {
    let component = components[0];
    let index = nodes
        .iter()
        .position(|node| node.component == component)
        .unwrap_or_else(|| {
            nodes.push(WorkspaceTreeNode {
                component,
                workspace: None,
                children: Vec::new(),
            });
            nodes.len() - 1
        });
    if components.len() == 1 {
        nodes[index].workspace = Some(workspace);
    } else {
        insert_tree_node(&mut nodes[index].children, &components[1..], workspace);
    }
}

fn append_tree_rows(
    nodes: &[WorkspaceTreeNode<'_>],
    ancestor_is_last: &mut Vec<bool>,
    rows: &mut Vec<Vec<Cell>>,
    include_resources: bool,
    usage_by_workspace: Option<&UsageIndex<'_>>,
    column_count: usize,
) {
    for (index, node) in nodes.iter().enumerate() {
        let is_last = index + 1 == nodes.len();
        let mut guide = tree_indent(ancestor_is_last);
        guide.push_str(if is_last { "└─ " } else { "├─ " });
        let mut name = node.component.to_owned();
        let mut visible_node = node;
        while visible_node.workspace.is_none() && visible_node.children.len() == 1 {
            visible_node = &visible_node.children[0];
            name.push('/');
            name.push_str(visible_node.component);
        }
        if visible_node.workspace.is_none() && !visible_node.children.is_empty() {
            name.push('/');
        }
        let workspace_cell =
            Cell::segmented([(guide, Tone::Dim), (name, Tone::Primary)], Tone::Primary);
        if let Some(workspace) = visible_node.workspace {
            rows.push(workspace_row(
                workspace,
                workspace_cell,
                false,
                include_resources,
                usage_by_workspace,
            ));
        } else {
            rows.push(grouping_row(workspace_cell, column_count));
        }
        ancestor_is_last.push(is_last);
        append_tree_rows(
            &visible_node.children,
            ancestor_is_last,
            rows,
            include_resources,
            usage_by_workspace,
            column_count,
        );
        ancestor_is_last.pop();
    }
}

fn tree_indent(ancestor_is_last: &[bool]) -> String {
    ancestor_is_last
        .iter()
        .map(|is_last| if *is_last { "   " } else { "│  " })
        .collect()
}

fn grouping_row(first: Cell, column_count: usize) -> Vec<Cell> {
    let mut row = Vec::with_capacity(column_count);
    row.push(first);
    row.resize_with(column_count, || Cell::new(String::new(), Tone::Primary));
    row
}

fn repository_groups(workspaces: &[WorkspaceListItem]) -> Vec<&[WorkspaceListItem]> {
    let mut groups = Vec::new();
    let mut start = 0;
    while start < workspaces.len() {
        let repository_id = &workspaces[start].repository.id;
        let mut end = start + 1;
        while end < workspaces.len() && workspaces[end].repository.id == *repository_id {
            end += 1;
        }
        groups.push(&workspaces[start..end]);
        start = end;
    }
    groups
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
    segments: Option<Vec<CellSegment>>,
}

#[derive(Debug, Clone)]
struct CellSegment {
    text: String,
    tone: Tone,
}

impl Cell {
    pub(super) fn new(text: String, tone: Tone) -> Self {
        Self {
            text: safe_line(&text),
            tone,
            segments: None,
        }
    }

    fn segmented<const N: usize>(segments: [(String, Tone); N], fallback_tone: Tone) -> Self {
        let segments = segments
            .into_iter()
            .map(|(text, tone)| CellSegment {
                text: safe_line(&text),
                tone,
            })
            .collect::<Vec<_>>();
        let text = segments
            .iter()
            .map(|segment| segment.text.as_str())
            .collect();
        Self {
            text,
            tone: fallback_tone,
            segments: Some(segments),
        }
    }

    fn render(&self, width: usize, last: bool, palette: Palette) -> String {
        let Some(segments) = &self.segments else {
            return palette.paint(self.tone, padded(&self.text, width, last));
        };
        if display_width(&self.text) > width {
            return palette.paint(self.tone, padded(&self.text, width, last));
        }
        let mut rendered = segments
            .iter()
            .map(|segment| palette.paint(segment.tone, &segment.text))
            .collect::<String>();
        if !last {
            rendered.push_str(&" ".repeat(width.saturating_sub(display_width(&self.text))));
        }
        rendered
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
            .map(|(index, cell)| cell.render(widths[index], index + 1 == row.len(), palette))
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
    use std::path::PathBuf;

    use super::*;
    use crate::domain::activity::{WorkspaceActivity, WorkspaceActivitySource};
    use crate::domain::runtime::{
        WorkspaceResourceScope, WorkspaceRuntimeBackend, WorkspaceRuntimeResources,
        WorkspaceRuntimeState,
    };
    use crate::domain::{
        ContextMode, ProfileSnapshot, Workspace, WorkspaceAvailability, WorkspaceLifecycle,
        WorkspacePhase, WorktreeMode,
    };
    use serde_json::json;

    fn workspace_item(
        repository: &str,
        repository_name: &str,
        name: &str,
        phase: WorkspacePhase,
    ) -> WorkspaceListItem {
        WorkspaceListItem {
            workspace: Workspace {
                id: format!("{repository}-{name}"),
                create_operation_id: None,
                repository_id: repository.to_owned(),
                name: name.to_owned(),
                context_mode: ContextMode::Fresh,
                context: json!({}),
                profile: ProfileSnapshot {
                    name: "default".to_owned(),
                    source_path: None,
                    source_hash: "profile-hash".to_owned(),
                    model_override: None,
                    effective_settings: json!({}),
                },
                lifecycle: WorkspaceLifecycle::Ready,
                availability: WorkspaceAvailability::Open,
                thread_runtime: None,
                phase,
                wait_reasons: Vec::new(),
                worktree_mode: WorktreeMode::NewBranch,
                branch_name: Some(format!("coco/{name}")),
                base_sha: Some("base".to_owned()),
                worktree_path: Some(PathBuf::from(format!("/worktrees/{name}"))),
                codex_thread_id: None,
                parent_thread_id: None,
                active_turn_id: None,
                last_error_code: None,
                last_error_message: None,
                created_at_ms: 1,
                updated_at_ms: 1,
                completed_at_ms: None,
                thread_archived: false,
                closed_head_sha: None,
                closed_at_ms: None,
            },
            repository: RepositorySummary {
                id: repository.to_owned(),
                display_name: repository_name.to_owned(),
                root_path: PathBuf::from(format!("/repos/{repository_name}")),
            },
            runtime_resources: None,
            activity: None,
        }
    }

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
    fn status_activity_extends_state_without_adding_another_column() {
        let mut workspace = workspace_item("repo", "project", "feat/login", WorkspacePhase::Active);
        workspace.activity = Some(WorkspaceActivity {
            label: "Checking tests".to_owned(),
            source: WorkspaceActivitySource::ReasoningSummary,
            thread_id: "thread-1".to_owned(),
            turn_id: "turn-1".to_owned(),
            item_id: "item-1".to_owned(),
            truncated: false,
            runtime_generation: "runtime-1".to_owned(),
            observed_at_ms: 1,
        });

        let rendered = render_workspace_list(
            &[workspace],
            false,
            false,
            None,
            usize::MAX,
            Palette::plain(),
        );
        assert!(rendered.contains("STATE"));
        assert!(!rendered.contains("ACTIVITY"));
        assert!(rendered.contains("● Working · Checking tests"));
    }

    #[test]
    fn tree_view_expands_shared_components_and_compacts_unique_paths() {
        let workspaces = vec![
            workspace_item(
                "repo",
                "project",
                "backend/services/api",
                WorkspacePhase::Active,
            ),
            workspace_item(
                "repo",
                "project",
                "backend/services/worker",
                WorkspacePhase::Idle,
            ),
            workspace_item("repo", "project", "docs/fix", WorkspacePhase::Idle),
            workspace_item("repo", "project", "frontend", WorkspacePhase::Idle),
            workspace_item(
                "repo",
                "project",
                "frontend/w1",
                WorkspacePhase::WaitingForInput,
            ),
            workspace_item("repo", "project", "frontend/w2", WorkspacePhase::Prepared),
        ];
        let rendered = render_workspace_tree(
            &workspaces,
            false,
            false,
            None,
            usize::MAX,
            Palette::plain(),
        );

        for expected in [
            "├─ backend/services/",
            "│  ├─ api",
            "│  └─ worker",
            "├─ docs/fix",
            "└─ frontend",
            "   ├─ w1",
            "   └─ w2",
            "Needs input",
        ] {
            assert!(
                rendered.contains(expected),
                "tree omitted {expected:?}:\n{rendered}"
            );
        }
        assert!(!rendered.contains("├─ backend/\n"));
        assert!(!rendered.contains("├─ docs/\n"));
    }

    #[test]
    fn tree_guides_are_dim_without_emphasizing_workspace_names() {
        let workspaces = vec![
            workspace_item("repo", "project", "backend/api", WorkspacePhase::Active),
            workspace_item("repo", "project", "backend/worker", WorkspacePhase::Idle),
        ];
        let palette = Palette::colored();
        let rendered = render_workspace_tree(&workspaces, false, false, None, usize::MAX, palette);
        let expected = format!(
            "{}{}",
            palette.paint(Tone::Dim, "└─ "),
            palette.paint(Tone::Primary, "backend/")
        );
        assert!(
            rendered.contains(&expected),
            "tree did not separate guide and workspace styling: {rendered:?}"
        );
        assert!(!rendered.contains(&palette.paint(Tone::Bold, "backend/")));
    }

    #[test]
    fn all_repository_tree_uses_path_sections_without_duplicate_names_and_remains_bounded() {
        let workspaces = vec![
            workspace_item("alpha", "alpha", "frontend/w2", WorkspacePhase::Idle),
            workspace_item("beta", "beta", "backend/api", WorkspacePhase::Active),
        ];
        let rendered = render_workspace_tree(&workspaces, true, false, None, 64, Palette::plain());

        assert!(rendered.starts_with("/repos/alpha\nWORKSPACE"));
        assert!(rendered.contains("└─ frontend/w2"));
        assert!(rendered.contains("\n\n/repos/beta\nWORKSPACE"));
        assert!(rendered.contains("└─ backend/api"));
        assert!(!rendered.contains("alpha · /repos/alpha"));
        assert_eq!(rendered.matches("/repos/alpha").count(), 1);
        assert_eq!(rendered.matches("/repos/beta").count(), 1);
        assert!(
            rendered.lines().all(|line| display_width(line) <= 64),
            "tree exceeded terminal width:\n{rendered}"
        );
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
