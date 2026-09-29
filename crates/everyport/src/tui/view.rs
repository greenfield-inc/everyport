//! Draws the TUI: a fixed top, a scrolling body and a footer of key hints,
//! laid out like the popover at up to 100 columns.

use super::chart::{sparkline, Canvas};
use super::{App, Page};
use crate::format;
use everyport::protocol::{AgentKind, CleanUpReason, Config, Server, ServerStatus, WorkspaceKind};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::sync::LazyLock;
use std::time::Instant;

/// The settings the thresholds and the empty list's port range come from.
static CONFIG: LazyLock<Config> = LazyLock::new(|| crate::commands::config().unwrap_or_default());

const HISTORY_MS: u64 = 10 * 60 * 1000;

/// Styles for the text levels. Without the terminal's colors, quiet levels are dim.
struct Styles {
    text1: Style,
    text2: Style,
    text3: Style,
    faint: Style,
    track: Style,
    highlight: Option<Color>,
}

fn level(color: Option<Color>, fallback: Style) -> Style {
    color.map_or(fallback, |c| Style::new().fg(c))
}

impl Styles {
    fn new(app: &App) -> Self {
        let p = &app.palette;
        let dim = Style::new().add_modifier(Modifier::DIM);
        Self {
            text1: Style::new().fg(p.text1()),
            text2: level(p.text2(), Style::new()),
            text3: level(p.text3(), dim),
            faint: level(p.faint(), dim),
            track: level(p.track(), dim),
            highlight: p.highlight(),
        }
    }
}

type Spans = Vec<Span<'static>>;

fn span(text: impl Into<String>, style: Style) -> Span<'static> {
    Span::styled(text.into(), style)
}

fn width(spans: &[Span]) -> usize {
    spans.iter().map(|s| s.width()).sum()
}

/// Cuts spans to `max` columns, ending in "…" when cut.
fn truncate(spans: Spans, max: usize) -> Spans {
    if width(&spans) <= max {
        return spans;
    }
    let mut out = Vec::new();
    let mut used = 0;
    for s in spans {
        let room = max.saturating_sub(1 + used);
        let text: String = s.content.chars().take(room).collect();
        used += text.chars().count();
        let cut = text.chars().count() < s.content.chars().count();
        out.push(Span::styled(text, s.style));
        if cut {
            out.push(Span::styled("…", s.style));
            break;
        }
    }
    out
}

/// `left` and `right` on one line, `width` wide. The left side gives way.
fn split(left: Spans, right: Spans, width_: usize) -> Spans {
    let left = truncate(left, width_.saturating_sub(width(&right) + 1));
    let gap = width_.saturating_sub(width(&left) + width(&right));
    let mut line = left;
    line.push(Span::raw(" ".repeat(gap)));
    line.extend(right);
    line
}

fn pad_left(text: &str, width: usize) -> String {
    format!("{text:>width$}")
}

struct Block {
    lines: Vec<Line<'static>>,
}

impl Block {
    fn new() -> Self {
        Self { lines: Vec::new() }
    }
    fn add(&mut self, spans: Spans) {
        self.lines.push(Line::from(spans));
    }
    fn blank(&mut self) {
        self.lines.push(Line::default());
    }
}

struct View<'a> {
    app: &'a App,
    s: Styles,
    width: usize,
    inner: usize,
    now: u64,
    /// End of the charts' ten-minute window.
    taken_at: u64,
}

pub fn draw(app: &mut App, frame: &mut Frame) {
    if app.toast.as_ref().is_some_and(|t| t.until < Instant::now()) {
        app.toast = None;
    }
    sync_selection(app);
    let area = frame.area();
    let width = (area.width as usize).min(100);
    let now = everyport::now_ms();
    let view = View {
        app,
        s: Styles::new(app),
        width,
        inner: width.saturating_sub(4).max(10),
        now,
        taken_at: app.snapshot.as_ref().map_or(now, |s| s.taken_at),
    };

    let (top, body, footer) = match app.page {
        Page::Servers => (
            view.servers_top(),
            view.servers_body(),
            view.servers_footer(),
        ),
        Page::Detail(port) => match app.server(port) {
            Some(server) => (
                view.detail_top(server),
                view.detail_body(server),
                view.detail_footer(server),
            ),
            None => (
                view.servers_top(),
                view.servers_body(),
                view.servers_footer(),
            ),
        },
        Page::Help => (
            view.help_top(),
            view.help_body(),
            view.hints(&[("↑ ↓", "Scroll"), ("esc", "Back")]),
        ),
    };
    let available = (area.height as usize).saturating_sub(top.lines.len() + footer.lines.len());
    let max_offset = body.lines.len().saturating_sub(available);
    let offset = match app.page {
        Page::Servers => list_offset(app, body.lines.len(), available),
        Page::Detail(_) => {
            app.detail_offset = app.detail_offset.min(max_offset);
            app.detail_offset
        }
        Page::Help => {
            app.help_offset = app.help_offset.min(max_offset);
            app.help_offset
        }
    };

    let mut lines = top.lines;
    lines.extend(body.lines.into_iter().skip(offset).take(available));
    // The footer sits at the bottom of the screen.
    lines.resize(
        area.height as usize - footer.lines.len().min(area.height as usize),
        Line::default(),
    );
    lines.extend(footer.lines);
    let area = Rect {
        width: width as u16,
        ..area
    };
    frame.render_widget(Paragraph::new(lines), area);
}

/// When the selected server goes away, selects the one that took its place.
fn sync_selection(app: &mut App) {
    if app.list_scrolling {
        return;
    }
    let ports: Vec<u16> = app.visible().iter().map(|s| s.port).collect();
    match ports.iter().position(|&p| Some(p) == app.selected) {
        Some(index) => app.selected_index = index,
        None => {
            app.selected = ports
                .get(app.selected_index.min(ports.len().saturating_sub(1)))
                .copied()
        }
    }
}

/// Keeps the selected row in view. The body is one blank line, then two
/// lines per server with a blank line between them.
fn list_offset(app: &mut App, lines: usize, available: usize) -> usize {
    if lines <= available {
        app.list_offset = 0;
        return 0;
    }
    let ports: Vec<u16> = app.visible().iter().map(|s| s.port).collect();
    if let Some(index) = ports
        .iter()
        .position(|&p| Some(p) == app.selected)
        .filter(|_| !app.list_scrolling)
    {
        let top = 1 + index * 3;
        if top < app.list_offset {
            app.list_offset = top - usize::from(index == 0);
        }
        if top + 2 > app.list_offset + available {
            app.list_offset = top + 2 - available + usize::from(index == ports.len() - 1);
        }
    }
    app.list_offset = app.list_offset.min(lines - available);
    app.list_offset
}

fn agent_glyph(kind: AgentKind) -> &'static str {
    match kind {
        AgentKind::ClaudeCode => "✳",
        AgentKind::Codex => ">_",
    }
}

impl View<'_> {
    /// Content starts two columns in, like the popover's 16 px inset.
    fn padded(&self, spans: Spans) -> Spans {
        let mut line = vec![Span::raw("  ")];
        line.extend(truncate(spans, self.inner));
        line
    }

    fn divider(&self) -> Spans {
        vec![span("─".repeat(self.width), self.s.track)]
    }

    fn centered(&self, spans: Spans) -> Spans {
        let left = self.inner.saturating_sub(width(&spans)) / 2;
        let mut line = vec![Span::raw(" ".repeat(left))];
        line.extend(spans);
        self.padded(line)
    }

    fn port_style(&self, port: u16) -> Style {
        Style::new().fg(self.app.palette.port(self.app.color_index(port)))
    }

    fn port_label(&self, server: &Server) -> Spans {
        let style = self.port_style(server.port);
        // The status slot, drawn like the socket mark's lit slot.
        let slot = match server.status {
            ServerStatus::Idle => self.s.faint,
            ServerStatus::Attention => style.add_modifier(Modifier::BOLD),
            ServerStatus::Running => style,
        };
        vec![
            span("▎", slot),
            span(server.port.to_string(), style.add_modifier(Modifier::BOLD)),
        ]
    }

    fn bold(&self) -> Style {
        self.s.text1.add_modifier(Modifier::BOLD)
    }

    fn amber(&self) -> Style {
        Style::new().fg(self.app.palette.amber())
    }

    // ------------------------------------------------------------ servers

    fn servers_top(&self) -> Block {
        let app = self.app;
        let mut block = Block::new();
        let title = match (app.cleaning, app.switcher()) {
            (true, _) => "Clean up".to_string(),
            (false, true) => format!("Servers · {}", app.label()),
            (false, false) => "Servers".to_string(),
        };
        block.add(self.centered(vec![span(title, self.bold())]));
        block.blank();

        let visible = app.visible();
        let (left, right) = if app.cleaning {
            let picked: Vec<&&Server> = visible
                .iter()
                .filter(|s| app.selection.contains(&s.port))
                .collect();
            let memory = picked.iter().map(|s| s.memory).sum();
            let note = if picked.is_empty() {
                "Pick servers to stop".to_string()
            } else {
                format!("freed by stopping {}", picked.len())
            };
            (format::total(memory), vec![span(note, self.s.text2)])
        } else {
            let memory = app.servers().iter().map(|s| s.memory).sum();
            let (label, percent) = if app.cpu_all {
                (
                    "CPU (all)",
                    app.snapshot.as_ref().map_or(0.0, |s| s.system.cpu_percent),
                )
            } else {
                let total: f32 = app.servers().iter().map(|s| s.cpu_percent).sum();
                ("CPU (servers)", total / app.cores as f32)
            };
            (
                format::total(memory),
                vec![
                    span(label, self.s.text3),
                    span(format!("  {}", format::percent(percent)), self.s.text2),
                ],
            )
        };
        let (number, unit) = left.split_once(' ').unwrap_or((&left, ""));
        let left = vec![
            span(number, self.bold()),
            span(format!(" {unit}"), self.s.text3),
        ];
        block.add(self.padded(split(left, right, self.inner)));
        block.add(self.padded(self.memory_bar()));
        block.add(self.padded(self.memory_legend()));
        block.add(self.divider());
        block
    }

    /// Servers as raised blocks in their port colors, other apps on a lower
    /// baseline, free memory as the track. The selected server stands taller.
    fn memory_bar(&self) -> Spans {
        let app = self.app;
        let Some(snapshot) = &app.snapshot else {
            return vec![span("▂".repeat(self.inner), self.s.track)];
        };
        let servers = app.visible();
        let columns = self.inner;
        let others = snapshot.system.memory_other_apps;
        let filled = servers.iter().map(|s| s.memory).sum::<u64>() + others;
        let capacity = snapshot.system.memory_total.max(filled).max(1) as f64;
        let mut server_columns: Vec<usize> = servers
            .iter()
            .map(|s| ((s.memory as f64 / capacity * columns as f64).round() as usize).max(1))
            .collect();
        let mut other_columns = (others as f64 / capacity * columns as f64).round() as usize;
        while server_columns.iter().sum::<usize>() + other_columns > columns {
            if other_columns > 0 {
                other_columns -= 1;
            } else if let Some(widest) =
                (0..server_columns.len()).max_by_key(|&i| server_columns[i])
            {
                if server_columns[widest] <= 1 {
                    break;
                }
                server_columns[widest] -= 1;
            } else {
                break;
            }
        }
        let mut line = Vec::new();
        for (server, count) in servers.iter().zip(server_columns) {
            let focused = Some(server.port) == app.selected;
            let dimmed = if app.cleaning {
                !app.selection.contains(&server.port) && !focused
            } else {
                !focused
            };
            let index = app.color_index(server.port);
            let style = if dimmed {
                app.palette.faded_port(index, 0.35)
            } else {
                Style::new().fg(app.palette.port(index))
            };
            line.push(span((if focused { "█" } else { "▅" }).repeat(count), style));
        }
        line.push(span("▂".repeat(other_columns), self.s.faint));
        let free = columns.saturating_sub(width(&line));
        line.push(span("▂".repeat(free), self.s.track));
        line
    }

    fn memory_legend(&self) -> Spans {
        let Some(snapshot) = &self.app.snapshot else {
            return Vec::new();
        };
        let system = &snapshot.system;
        let servers: u64 = self.app.servers().iter().map(|s| s.memory).sum();
        let mut line = vec![
            span("▅ ", self.s.text1),
            span("Servers ", self.s.text2),
            span(format::total(servers), self.s.text3),
            span("   ▂ ", self.s.faint),
            span("Other apps ", self.s.text2),
            span(format::total(system.memory_other_apps), self.s.text3),
        ];
        if system.memory_total > 0 {
            let free = system.memory_total.saturating_sub(system.memory_used);
            let free = format::total(free);
            line.extend([
                span("   ▂ ", self.s.track),
                span("Free ", self.s.text2),
                span(
                    format!(
                        "{} of {}",
                        free.split(' ').next().unwrap_or_default(),
                        format::total(system.memory_total)
                    ),
                    self.s.text3,
                ),
            ]);
        }
        line
    }

    fn servers_body(&self) -> Block {
        let app = self.app;
        let mut block = Block::new();
        let servers = app.visible();
        let others = app
            .snapshot
            .as_ref()
            .map_or(&[][..], |s| s.other_ports.as_slice());
        if servers.is_empty() && (others.is_empty() || app.cleaning) {
            let config = &*CONFIG;
            block.blank();
            for _ in 0..5 {
                block.add(self.centered(vec![span("● ● ● ● ●", self.s.faint)]));
            }
            block.blank();
            let (title, error) = if app.cleaning {
                ("No servers to clean up".to_string(), false)
            } else if app.snapshot.is_some() {
                ("Nothing listening".to_string(), false)
            } else {
                app.waiting()
            };
            let style = if error {
                Style::new().fg(app.palette.red())
            } else {
                self.s.text2
            };
            block.add(self.centered(vec![span(title, style)]));
            block.add(self.centered(vec![span(
                format!(
                    "Dev servers on ports {}-{} show up here.",
                    config.min_port, config.max_port
                ),
                self.s.text3,
            )]));
            return block;
        }
        block.blank();
        for (index, server) in servers.iter().enumerate() {
            if index > 0 {
                block.blank();
            }
            for line in self.server_row(server, Some(server.port) == app.selected) {
                block.add(line);
            }
        }
        if !others.is_empty() && !app.cleaning {
            block.blank();
            block.add(self.padded(vec![span("Other ports", self.bold())]));
            for port in others {
                block.add(self.padded(vec![span(format::other_port(port), self.s.text2)]));
            }
        }
        block.blank();
        block
    }

    fn server_row(&self, server: &Server, selected: bool) -> [Spans; 2] {
        let app = self.app;
        let attention = server.status == ServerStatus::Attention;
        let mut first = Vec::new();
        let mut second = Vec::new();
        if app.cleaning {
            let on = app.selection.contains(&server.port);
            first.push(span(
                if on { "[✓] " } else { "[ ] " },
                if on { self.bold() } else { self.s.text3 },
            ));
            second.push(Span::raw("    "));
        }
        first.extend(self.port_label(server));
        first.push(Span::raw(
            " ".repeat(
                7usize
                    .saturating_sub(1 + server.port.to_string().len())
                    .max(1),
            ),
        ));
        second.push(Span::raw("       "));
        let name = server
            .project
            .branch
            .as_deref()
            .unwrap_or(&server.project.name)
            .replace('-', " ");
        first.push(span(name, self.bold()));

        let values: Vec<f64> = server.history.iter().map(|s| s.memory as f64).collect();
        let quiet = if attention {
            self.amber()
        } else {
            self.s.text2
        };
        let right = vec![
            span(sparkline(&values, 8), quiet),
            span(
                pad_left(&format::bytes(server.memory), 9),
                if attention {
                    self.amber()
                } else {
                    self.s.text1
                },
            ),
        ];
        let content = self.inner;
        let line_one = split(first, right, content);
        let context_width = content.saturating_sub(width(&second));
        second.extend(self.context(server, context_width));
        let mut lines = [line_one, truncate(second, content)];
        if !server.cwd_exists && server.status == ServerStatus::Idle {
            for line in &mut lines {
                for s in line.iter_mut() {
                    s.style = s.style.add_modifier(Modifier::DIM);
                }
            }
        }
        let highlight = if selected { self.s.highlight } else { None };
        let mut first = true;
        lines.map(|line| {
            let marker = if selected && highlight.is_none() && std::mem::take(&mut first) {
                "›"
            } else {
                " "
            };
            let mut row = vec![Span::raw(" "), span(marker, self.bold())];
            row.extend(line);
            row.push(Span::raw(" "));
            if let Some(color) = highlight {
                // Fill the row's background from the marker to the right edge.
                let fill = (self.inner + 2).saturating_sub(width(&row) - 1);
                row.push(Span::raw(" ".repeat(fill)));
                for s in row.iter_mut().skip(1) {
                    s.style = s.style.bg(color);
                }
            }
            row
        })
    }

    fn context(&self, server: &Server, width_: usize) -> Spans {
        if self.app.cleaning {
            if let Some(reason) = &server.clean_up {
                let style = if matches!(reason, CleanUpReason::Leaking { .. }) {
                    self.amber()
                } else {
                    self.s.text2
                };
                return vec![span(format::reason(reason), style)];
            }
        }
        if server.status == ServerStatus::Attention {
            if let Some(CleanUpReason::Leaking { bytes }) = server.clean_up {
                let span_ms = match (server.history.first(), server.history.last()) {
                    (Some(a), Some(b)) => b.at.saturating_sub(a.at),
                    _ => 0,
                };
                return vec![span(
                    format!(
                        "+{} in {}",
                        format::bytes(bytes),
                        format::duration(span_ms / 1000)
                    ),
                    self.amber(),
                )];
            }
            let threshold = CONFIG.alert_memory as f64 / 1_073_741_824.0;
            return vec![span(format!("Over {} GB", trim(threshold)), self.amber())];
        }
        let mut line = Vec::new();
        if let Some(agent) = &server.agent {
            line.push(span(format!("{} ", agent_glyph(agent.kind)), self.s.text2));
        }
        let time = self.time_text(server);
        if server.cwd_exists && server.project.branch.is_some() {
            let suffix = format!(" · {time}");
            let room = width_
                .saturating_sub(width(&line) + suffix.chars().count())
                .max(1);
            line.extend(truncate(
                vec![span(server.project.name.replace('-', " "), self.s.text2)],
                room,
            ));
            line.push(span(suffix, self.s.text2));
        } else {
            let location = self.location(server);
            let text = [location, time]
                .into_iter()
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            line.push(span(text, self.s.text2));
        }
        line
    }

    /// Uptime, or idle time once idle for over an hour.
    fn time_text(&self, server: &Server) -> String {
        let idle = format::idle(server, self.now);
        if !server.cwd_exists {
            return format!("Worktree deleted · idle {}", format::short_duration(idle));
        }
        if idle > 3600 {
            format!("idle {}", format::short_duration(idle))
        } else {
            format::uptime(server, self.now).map_or(String::new(), |up| {
                format!("up {}", format::short_duration(up))
            })
        }
    }

    fn location(&self, server: &Server) -> String {
        if !server.cwd_exists {
            return String::new();
        }
        if let Some(workspace) = &server.workspace {
            return workspace.name.clone();
        }
        if let Some(worktree) = &server.project.worktree {
            return worktree.clone();
        }
        match server.project.root.as_ref().or(server.cwd.as_ref()) {
            Some(root) => tilde(parent(root)),
            None => server.project.name.clone(),
        }
    }

    fn servers_footer(&self) -> Block {
        let app = self.app;
        if let Some(block) = self.message_footer() {
            return block;
        }
        if app.cleaning {
            let picked: Vec<&Server> = app
                .visible()
                .into_iter()
                .filter(|s| app.selection.contains(&s.port))
                .collect();
            let label = if picked.is_empty() {
                "Stop servers".to_string()
            } else {
                let memory = picked.iter().map(|s| s.memory).sum();
                format!(
                    "Stop {} · free {}",
                    format::plural(picked.len(), "server", "servers"),
                    format::bytes(memory)
                )
            };
            return self.hints(&[
                ("space", "Select"),
                ("a", "All"),
                ("⏎", &label),
                ("esc", "Cancel"),
            ]);
        }
        let machine: &[(&str, &str)] = if app.switcher() {
            &[("tab", "Machine")]
        } else {
            &[]
        };
        if app.servers().is_empty() {
            return self.hints(
                &[
                    machine,
                    &[("pgup pgdn", "Scroll"), ("?", "Keys"), ("q", "Quit")],
                ]
                .concat(),
            );
        }
        let count = app
            .servers()
            .iter()
            .filter(|s| format::preselected(s))
            .count();
        let clean = if count > 0 {
            format!("Clean up {count}")
        } else {
            "Clean up".into()
        };
        self.hints(
            &[
                &[
                    ("⏎", "Details"),
                    ("o", "Open"),
                    ("s", "Stop"),
                    ("r", "Restart"),
                    ("c", &clean),
                ],
                machine,
                &[("?", "Keys"), ("q", "Quit")],
            ]
            .concat(),
        )
    }

    // ------------------------------------------------------------ detail

    fn detail_top(&self, server: &Server) -> Block {
        let mut block = Block::new();
        let name = &server.project.name;
        let left = (self.inner.saturating_sub(name.chars().count()) / 2).max(2);
        block.add(self.padded(vec![
            span("‹", self.s.text2),
            Span::raw(" ".repeat(left - 1)),
            span(name.clone(), self.bold()),
        ]));
        block.blank();
        let running = format::uptime(server, self.now).map_or(Vec::new(), |up| {
            vec![span(
                format!("Running for {}", format::duration(up)),
                self.s.text3,
            )]
        });
        block.add(self.padded(split(self.port_label(server), running, self.inner)));
        block.add(self.divider());
        block
    }

    fn detail_body(&self, server: &Server) -> Block {
        let app = self.app;
        let label_width = 13;
        let value_width = self.inner.saturating_sub(label_width);
        let row = |label: &str, value: Spans| -> Spans {
            let mut line = vec![span(format!("{label:<label_width$}"), self.s.text3)];
            line.extend(truncate(value, value_width));
            self.padded(line)
        };
        let middle = |text: &str| middle(text, value_width);

        let mut primary = Vec::new();
        if let Some(agent) = &server.agent {
            let title = agent
                .title
                .clone()
                .unwrap_or_else(|| format::agent(agent.kind).into());
            primary.push(row(
                "Session",
                vec![span(
                    format!("{} {title}", agent_glyph(agent.kind)),
                    self.s.text1,
                )],
            ));
        }
        if let Some(branch) = &server.project.branch {
            primary.push(row("Branch", vec![span(branch.clone(), self.s.text1)]));
        }
        let folder = server.cwd.as_deref().map(folder);
        let folder_is_primary = primary.len() < 2;
        if folder_is_primary {
            if let Some(folder) = &folder {
                primary.push(row("Folder", vec![span(middle(folder), self.s.text2)]));
            }
        }

        let mut secondary = Vec::new();
        if let Some(workspace) = &server.workspace {
            let kind = match workspace.kind {
                WorkspaceKind::Conductor => "Conductor",
                WorkspaceKind::Pane => "Pane",
                WorkspaceKind::GitWorktree => "Git worktree",
            };
            secondary.push(row(
                "Workspace",
                vec![span(format!("{kind} · {}", workspace.name), self.s.text2)],
            ));
        }
        if !folder_is_primary {
            if let Some(folder) = &folder {
                secondary.push(row("Folder", vec![span(middle(folder), self.s.text2)]));
            }
        }
        if let Some(framework) = &server.project.framework {
            secondary.push(row(
                "Framework",
                vec![span(framework.clone(), self.s.text2)],
            ));
        }
        if let Some(command) = &server.command {
            secondary.push(row("Command", vec![span(middle(command), self.s.text2)]));
        }
        if let Some(preview) = server
            .project
            .vercel
            .as_ref()
            .and_then(|v| v.preview_url.as_ref())
        {
            secondary.push(row("Preview", vec![span(middle(preview), self.s.text2)]));
        }
        if let Some(agent) = &server.agent {
            secondary.push(row(
                "Session ID",
                vec![span(middle(&agent.id), self.s.text2)],
            ));
        }
        if !server.addresses.is_empty() {
            secondary.push(row(
                "Address",
                vec![span(middle(&server.addresses.join(" · ")), self.s.text2)],
            ));
        }

        let mut block = Block::new();
        block.blank();
        primary.into_iter().for_each(|l| block.add(l));
        let more = secondary.len();
        if app.info_expanded {
            secondary.into_iter().for_each(|l| block.add(l));
        }
        if more > 0 {
            let label = if app.info_expanded {
                "Less ▴".to_string()
            } else {
                format!("{more} more ▾")
            };
            block.add(self.padded(vec![
                Span::raw(" ".repeat(label_width)),
                span(label, self.s.text3),
                span("   i", self.s.text2.add_modifier(Modifier::BOLD)),
            ]));
        }
        block.blank();
        block.add(self.divider());

        block.blank();
        block.add(self.padded(split(
            vec![
                span("Memory", self.s.text2),
                span(format!("  {}", format::bytes(server.memory)), self.bold()),
            ],
            vec![span("10 min", self.s.text3)],
            self.inner,
        )));
        self.memory_chart(server)
            .into_iter()
            .for_each(|l| block.add(self.padded(l)));
        block.blank();
        block.add(self.padded(vec![
            span("CPU", self.s.text2),
            span(
                format!("  {}", format::percent(server.cpu_percent)),
                self.bold(),
            ),
        ]));
        self.cpu_chart(server)
            .into_iter()
            .for_each(|l| block.add(self.padded(l)));
        if server.history.len() < 2 {
            block.add(self.padded(vec![span(
                "History fills in while Everyport runs.",
                self.s.text3,
            )]));
        }
        block.blank();
        block.add(self.divider());

        block.blank();
        let summary = format!(
            "{} · {}",
            server.processes.len(),
            format::bytes(server.memory)
        );
        block.add(self.padded(split(
            vec![
                span("Processes ", self.s.text2),
                span(if app.processes_expanded { "▾" } else { "▸" }, self.s.text3),
                span("   p", self.s.text2.add_modifier(Modifier::BOLD)),
            ],
            vec![span(summary, self.s.text2)],
            self.inner,
        )));
        if app.processes_expanded {
            let largest = server
                .processes
                .iter()
                .map(|p| p.memory)
                .max()
                .unwrap_or(1)
                .max(1);
            for process in &server.processes {
                let depth = process.depth as usize;
                let tree = if depth > 0 {
                    format!("{}└ {}", "  ".repeat(depth - 1), process.name)
                } else {
                    process.name.clone()
                };
                let listener = process.proc.pid == server.pid;
                let filled =
                    ((8.0 * process.memory as f64 / largest as f64).round() as usize).clamp(1, 8);
                let style = if listener { self.s.text1 } else { self.s.text2 };
                let right = vec![
                    span(format!("{:<8}", process.proc.pid), self.s.text3),
                    span("━".repeat(filled), style),
                    span("─".repeat(8 - filled), self.s.track),
                    span(pad_left(&format::bytes(process.memory), 10), self.s.text1),
                ];
                block.add(self.padded(split(vec![span(tree, style)], right, self.inner)));
            }
        }
        block.blank();
        block
    }

    /// Line chart over the last ten minutes, with the alert threshold dotted in amber.
    fn memory_chart(&self, server: &Server) -> Vec<Spans> {
        const GB: f64 = 1_073_741_824.0;
        let threshold = CONFIG.alert_memory as f64 / GB;
        let top = server
            .history
            .iter()
            .map(|s| s.memory as f64 / GB)
            .fold(0.0, f64::max)
            * 1.1;
        let top = top.max(threshold).max(0.001);
        let threshold_label = format!("{} GB", trim(threshold));
        let gutter = threshold_label.chars().count() + 1;
        let rows = 4;
        let mut canvas = Canvas::new(self.inner.saturating_sub(gutter + 1), rows);
        let dot_height = canvas.dot_height() as f64;
        let y = |gb: f64| ((1.0 - gb / top) * (dot_height - 1.0)).round() as i64;
        let points = self.timed_points(server, canvas.dot_width(), |s| y(s.memory as f64 / GB));
        canvas.plot(&points);

        let mut marks = Canvas::new(canvas.columns, rows);
        let threshold_y = y(threshold);
        for x in (0..marks.dot_width() as i64).step_by(2) {
            marks.set(x, threshold_y);
        }
        let threshold_row = (threshold_y / 4) as usize;
        (0..rows)
            .map(|row| {
                let (label, style) = if row == threshold_row {
                    (threshold_label.as_str(), self.app.palette.faded_amber(0.85))
                } else if row == rows - 1 {
                    ("0", self.s.text3)
                } else {
                    ("", self.s.text3)
                };
                let mut line = vec![
                    span(format!("{} ", pad_left(label, gutter - 1)), style),
                    span("│", self.s.track),
                ];
                let mark = self.app.palette.faded_amber(0.6);
                for column in 0..canvas.columns {
                    if !canvas.is_empty(row, column) {
                        line.push(span(canvas.char(row, column).to_string(), self.s.text1));
                    } else if !marks.is_empty(row, column) {
                        line.push(span(marks.char(row, column).to_string(), mark));
                    } else {
                        line.push(Span::raw(" "));
                    }
                }
                line
            })
            .collect()
    }

    /// Bars over the last ten minutes, the latest one brightest.
    fn cpu_chart(&self, server: &Server) -> Vec<Spans> {
        let rows = 4;
        let gutter = 5;
        let columns = self.inner.saturating_sub(gutter + 1).max(1);
        let start = self.taken_at.saturating_sub(HISTORY_MS);
        let mut buckets: Vec<Vec<f32>> = vec![Vec::new(); columns];
        for sample in &server.history {
            let fraction = sample.at.saturating_sub(start) as f64 / HISTORY_MS as f64;
            let index = ((fraction * columns as f64) as usize).min(columns - 1);
            buckets[index].push(sample.cpu_percent.min(100.0));
        }
        let latest = buckets.iter().rposition(|b| !b.is_empty());
        const BLOCKS: [&str; 9] = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
        (0..rows)
            .map(|row| {
                let label = match row {
                    0 => "100%",
                    r if r == rows - 1 => "0",
                    _ => "",
                };
                let mut line = vec![
                    span(format!("{} ", pad_left(label, gutter - 1)), self.s.text3),
                    span("│", self.s.track),
                ];
                for (index, bucket) in buckets.iter().enumerate() {
                    if bucket.is_empty() {
                        line.push(Span::raw(" "));
                        continue;
                    }
                    let value = bucket.iter().sum::<f32>() / bucket.len() as f32;
                    let mut eighths = (value / 100.0 * (rows * 8) as f32).round() as i64;
                    if value >= 1.0 {
                        eighths = eighths.max(1);
                    }
                    let from_bottom = (rows - 1 - row) as i64;
                    let level = (eighths - from_bottom * 8).clamp(0, 8) as usize;
                    let style = if Some(index) == latest {
                        self.s.text1
                    } else {
                        self.s.faint
                    };
                    line.push(span(BLOCKS[level], style));
                }
                line
            })
            .collect()
    }

    /// Positions history samples across `dots` columns spanning the last ten minutes.
    fn timed_points(
        &self,
        server: &Server,
        dots: usize,
        y: impl Fn(&everyport::protocol::Sample) -> i64,
    ) -> Vec<(i64, i64)> {
        let start = self.taken_at.saturating_sub(HISTORY_MS);
        let mut points: Vec<(i64, i64)> = Vec::new();
        for sample in &server.history {
            let fraction = sample.at.saturating_sub(start) as f64 / HISTORY_MS as f64;
            let x = ((fraction * (dots.saturating_sub(1)) as f64).round() as i64)
                .clamp(0, dots as i64 - 1);
            match points.last_mut() {
                Some(last) if last.0 == x => last.1 = y(sample),
                _ => points.push((x, y(sample))),
            }
        }
        points
    }

    fn detail_footer(&self, server: &Server) -> Block {
        if let Some(block) = self.message_footer() {
            return block;
        }
        let open = format!("Open localhost:{}", server.port);
        let info = if self.app.info_expanded {
            "Less info"
        } else {
            "More info"
        };
        let processes = if self.app.processes_expanded {
            "Hide processes"
        } else {
            "Processes"
        };
        self.hints(&[
            ("⏎", &open),
            ("s", "Stop"),
            ("r", "Restart"),
            ("i", info),
            ("p", processes),
            ("?", "Keys"),
            ("esc", "Back"),
        ])
    }

    // ------------------------------------------------------------ footer and help

    fn message_footer(&self) -> Option<Block> {
        let app = self.app;
        let mut block = Block::new();
        block.add(self.divider());
        if let Some(server) = app.confirming_stop.and_then(|port| app.server(port)) {
            let red = Style::new().fg(app.palette.red());
            let port = span(
                format!(":{}", server.port),
                self.port_style(server.port).add_modifier(Modifier::BOLD),
            );
            let question = if server.protected {
                vec![
                    span(format!("{} ", server.process_name), self.s.text1),
                    port,
                    span(" is protected. Stop it anyway?", self.s.text1),
                ]
            } else {
                vec![
                    span(format!("Stop {} ", server.project.name), self.s.text1),
                    port,
                    span(
                        format!(
                            " and its {}?",
                            format::plural(server.processes.len(), "process", "processes")
                        ),
                        self.s.text1,
                    ),
                ]
            };
            let keys = vec![
                span("y", red.add_modifier(Modifier::BOLD)),
                span(" Stop   ", red),
                span("n", self.bold()),
                span(" Cancel", self.s.text3),
            ];
            block.add(self.padded(split(question, keys, self.inner)));
        } else if let Some(toast) = &app.toast {
            let style = if toast.error {
                Style::new().fg(app.palette.red())
            } else {
                self.s.text2
            };
            block.add(self.padded(vec![span(toast.text.clone(), style)]));
        } else {
            return None;
        }
        block.blank();
        Some(block)
    }

    /// Key hints. Those that don't fit drop from the end, keeping Keys as long
    /// as possible and always the last one (Quit or Back).
    fn hints(&self, hints: &[(&str, &str)]) -> Block {
        let pieces: Vec<Spans> = hints
            .iter()
            .map(|(key, label)| {
                vec![
                    span(*key, self.bold()),
                    span(format!(" {label}"), self.s.text3),
                ]
            })
            .collect();
        let mut kept: Vec<bool> = vec![true; hints.len()];
        let total = |kept: &[bool]| {
            let widths: Vec<usize> = pieces
                .iter()
                .zip(kept)
                .filter(|(_, k)| **k)
                .map(|(p, _)| width(p))
                .collect();
            widths.iter().sum::<usize>() + widths.len().saturating_sub(1) * 3
        };
        let optional: Vec<usize> = (0..hints.len().saturating_sub(1)).rev().collect();
        for &index in &optional {
            if hints[index].0 != "?" && total(&kept) > self.inner {
                kept[index] = false;
            }
        }
        for &index in &optional {
            if total(&kept) > self.inner {
                kept[index] = false;
            }
        }
        let mut line = Vec::new();
        for (piece, _) in pieces.into_iter().zip(&kept).filter(|(_, k)| **k) {
            if !line.is_empty() {
                line.push(Span::raw("   "));
            }
            line.extend(piece);
        }
        let mut block = Block::new();
        block.add(self.divider());
        block.add(self.padded(line));
        block.blank();
        block
    }

    fn help_top(&self) -> Block {
        let mut block = Block::new();
        block.add(self.centered(vec![span("Keys", self.bold())]));
        block.blank();
        block.add(self.divider());
        block
    }

    fn help_body(&self) -> Block {
        let groups: [(&str, &[(&str, &str)]); 5] = [
            (
                "Servers",
                &[
                    ("↑ ↓  j k", "Select"),
                    ("pgup pgdn", "Scroll list"),
                    ("⏎", "Details"),
                    ("c", "Clean up"),
                    ("t", "CPU for servers or the whole machine"),
                    ("tab  m", "Next machine"),
                ],
            ),
            (
                "Actions",
                &[
                    ("o", "Open in browser"),
                    ("v", "Vercel preview"),
                    ("r", "Restart"),
                    ("s", "Stop"),
                ],
            ),
            (
                "Clean up",
                &[
                    ("space", "Select or deselect"),
                    ("a", "Select all"),
                    ("⏎", "Stop selected"),
                    ("esc", "Cancel"),
                ],
            ),
            (
                "Details",
                &[
                    ("⏎", "Open in browser"),
                    ("i", "More or less info"),
                    ("p", "Show or hide processes"),
                    ("tab", "Next server"),
                    ("↑ ↓", "Scroll"),
                    ("esc", "Back"),
                ],
            ),
            (
                "Anywhere",
                &[
                    ("?", "Keys"),
                    ("esc", "Back, or quit from the list"),
                    ("q", "Quit"),
                ],
            ),
        ];
        let mut block = Block::new();
        for (title, keys) in groups {
            block.blank();
            block.add(self.padded(vec![span(title, self.s.text2)]));
            for (key, label) in keys {
                block.add(self.padded(vec![
                    span(format!("{key:<13}"), self.bold()),
                    span(*label, self.s.text3),
                ]));
            }
        }
        block.blank();
        block
    }
}

/// "2" for 2.0, "1.5" for 1.5.
fn trim(value: f64) -> String {
    let text = format!("{value:.1}");
    text.strip_suffix(".0").map_or(text.clone(), String::from)
}

fn parent(path: &str) -> &str {
    path.trim_end_matches(['/', '\\'])
        .rsplit_once(['/', '\\'])
        .map_or(path, |(parent, _)| parent)
}

fn tilde(path: &str) -> String {
    match dirs::home_dir().and_then(|home| {
        path.strip_prefix(&*home.to_string_lossy())
            .map(String::from)
    }) {
        Some(rest) => format!("~{rest}"),
        None => path.to_string(),
    }
}

/// "~/…/parent/folder" for deep paths.
fn folder(path: &str) -> String {
    let short = tilde(path);
    let root = if short.starts_with('/') { "/" } else { "" };
    let parts: Vec<&str> = short.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    if parts.len() <= 3 {
        return short;
    }
    format!(
        "{root}{}/…/{}",
        parts[0],
        parts[parts.len() - 2..].join("/")
    )
}

/// Cuts the middle of `text` to fit `width`, keeping both ends.
fn middle(text: &str, width: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= width || width < 3 {
        return text.to_string();
    }
    let keep = width - 1;
    let head = keep / 2;
    let tail = keep - head;
    format!(
        "{}…{}",
        chars[..head].iter().collect::<String>(),
        chars[chars.len() - tail..].iter().collect::<String>()
    )
}
