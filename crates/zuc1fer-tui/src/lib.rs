use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, Gauge, List, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Tabs,
    },
    Frame,
};
use std::cell::Cell;
use std::collections::VecDeque;
use tui_textarea::TextArea;

pub struct App {
    pub messages: VecDeque<Message>,
    pub input: TextArea<'static>,
    pub status: String,
    pub model: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub context_max_tokens: u64,
    pub running: bool,
    pub streaming: bool,
    pub repo_files: Vec<(String, f64)>,
    pub show_repo_panel: bool,
    pub sidebar_tab: usize,
    pub mcp_servers: Vec<(String, bool)>,
    pub cost_usd: f64,
    pub palette_open: bool,
    pub palette_query: String,
    pub palette_selection: usize,
    last_assistant_idx: usize,
    scroll_offset: Cell<usize>,
    auto_scroll: Cell<bool>,
    total_lines: Cell<usize>,
    view_height: Cell<usize>,
    scrollbar_state: Cell<ScrollbarState>,
}

pub struct Message {
    pub role: MessageRole,
    pub text: String,
}

pub enum MessageRole {
    User,
    Assistant,
    System,
}

impl App {
    pub fn new(model: &str) -> Self {
        let mut input = TextArea::default();
        input.set_block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
        input.set_style(Style::default().fg(Color::White));
        input.set_cursor_line_style(Style::default());

        Self {
            messages: VecDeque::new(),
            input,
            status: String::from("Ready"),
            model: model.to_string(),
            tokens_in: 0,
            tokens_out: 0,
            context_max_tokens: 131072,
            running: true,
            streaming: false,
            repo_files: Vec::new(),
            show_repo_panel: false,
            sidebar_tab: 0,
            mcp_servers: Vec::new(),
            cost_usd: 0.0,
            palette_open: false,
            palette_query: String::new(),
            palette_selection: 0,
            last_assistant_idx: 0,
            scroll_offset: Cell::new(0),
            auto_scroll: Cell::new(true),
            total_lines: Cell::new(0),
            view_height: Cell::new(24),
            scrollbar_state: Cell::new(ScrollbarState::default()),
        }
    }

    pub fn add_user_message(&mut self, text: String) {
        self.messages.push_back(Message {
            role: MessageRole::User,
            text,
        });
    }

    pub fn add_system_message(&mut self, text: String) {
        self.messages.push_back(Message {
            role: MessageRole::System,
            text,
        });
    }

    pub fn add_message(&mut self, text: String) {
        self.messages.push_back(Message {
            role: MessageRole::Assistant,
            text,
        });
    }

    pub fn start_streaming(&mut self) {
        let idx = self.messages.len();
        self.messages.push_back(Message {
            role: MessageRole::Assistant,
            text: String::new(),
        });
        self.last_assistant_idx = idx;
        self.streaming = true;
        self.auto_scroll.set(true);
    }

    pub fn append_stream(&mut self, text: &str) {
        if let Some(msg) = self.messages.get_mut(self.last_assistant_idx) {
            msg.text.push_str(text);
        }
    }

    pub fn end_streaming(&mut self) {
        self.streaming = false;
    }

    pub fn next_turn(&mut self) {
        let idx = self.messages.len();
        self.messages.push_back(Message {
            role: MessageRole::Assistant,
            text: String::new(),
        });
        self.last_assistant_idx = idx;
    }

    pub fn take_input(&mut self) -> String {
        let text = self.input.lines().join("\n").trim().to_string();
        self.input = TextArea::default();
        self.input.set_block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
        self.input.set_style(Style::default().fg(Color::White));
        self.input.set_cursor_line_style(Style::default());
        text
    }

    pub fn update_cost(&mut self) {
        let provider = self.model.split('/').next().unwrap_or("unknown");
        let (price_in, price_out) = model_pricing(provider);
        let in_cost = (self.tokens_in as f64 / 1_000_000.0) * price_in;
        let out_cost = (self.tokens_out as f64 / 1_000_000.0) * price_out;
        self.cost_usd = in_cost + out_cost;
    }

    fn selected_palette_command(&self) -> String {
        let commands = palette_commands();
        let q = self.palette_query.to_lowercase();
        let filtered: Vec<&(&str, &str)> = if q.is_empty() {
            commands.iter().collect()
        } else {
            commands
                .iter()
                .filter(|(cmd, desc)| {
                    cmd.to_lowercase().contains(&q) || desc.to_lowercase().contains(&q)
                })
                .collect()
        };
        let idx = self.palette_selection.min(filtered.len().saturating_sub(1));
        filtered
            .get(idx)
            .map(|(cmd, _)| cmd.to_string())
            .unwrap_or_else(|| self.palette_query.clone())
    }

    fn max_scroll(&self) -> usize {
        self.total_lines
            .get()
            .saturating_sub(self.view_height.get())
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<String> {
        if self.palette_open {
            match key.code {
                KeyCode::Esc => {
                    self.palette_open = false;
                    self.palette_query.clear();
                    self.palette_selection = 0;
                }
                KeyCode::Enter => {
                    let cmd = self.selected_palette_command();
                    self.palette_open = false;
                    self.palette_query.clear();
                    self.palette_selection = 0;
                    return Some(cmd);
                }
                KeyCode::Up => {
                    self.palette_selection = self.palette_selection.saturating_sub(1);
                }
                KeyCode::Down => {
                    self.palette_selection += 1;
                }
                KeyCode::Backspace => {
                    self.palette_query.pop();
                    self.palette_selection = 0;
                }
                KeyCode::Char(c) => {
                    self.palette_query.push(c);
                    self.palette_selection = 0;
                }
                _ => {}
            }
            return None;
        }

        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.running = false;
            }
            KeyCode::Char('/') if !self.streaming => {
                self.palette_open = true;
                self.palette_query.clear();
                self.palette_selection = 0;
            }
            KeyCode::Tab => {
                if self.show_repo_panel {
                    self.sidebar_tab = (self.sidebar_tab + 1) % 3;
                } else {
                    self.show_repo_panel = true;
                }
            }
            KeyCode::Esc => {
                if self.show_repo_panel {
                    self.show_repo_panel = false;
                }
            }
            KeyCode::PageUp => {
                let page = self.view_height.get().saturating_sub(2);
                self.scroll_offset
                    .set(self.scroll_offset.get().saturating_sub(page));
                self.auto_scroll.set(false);
            }
            KeyCode::PageDown => {
                let page = self.view_height.get().saturating_sub(2);
                let max = self.max_scroll();
                let new = (self.scroll_offset.get() + page).min(max);
                self.scroll_offset.set(new);
                if new >= max {
                    self.auto_scroll.set(true);
                }
            }
            KeyCode::Up => {
                self.scroll_offset
                    .set(self.scroll_offset.get().saturating_sub(1));
                self.auto_scroll.set(false);
            }
            KeyCode::Down => {
                let max = self.max_scroll();
                let new = (self.scroll_offset.get() + 1).min(max);
                self.scroll_offset.set(new);
                if new >= max {
                    self.auto_scroll.set(true);
                }
            }
            _ => {
                if !self.streaming {
                    self.input.input(key);
                }
            }
        }
        None
    }

    pub fn handle_mouse_scroll(&self, direction: i16) {
        if direction > 0 {
            let max = self.max_scroll();
            let new = (self.scroll_offset.get() + 3).min(max);
            self.scroll_offset.set(new);
            if new >= max {
                self.auto_scroll.set(true);
            }
        } else {
            self.scroll_offset
                .set(self.scroll_offset.get().saturating_sub(3));
            self.auto_scroll.set(false);
        }
    }
}

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Min(4),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(frame, chunks[0], app);

    let status_text = if app.streaming {
        " ● Streaming..."
    } else if app.status != "Ready" {
        &app.status
    } else {
        ""
    };
    let status_style = if app.streaming || app.status != "Ready" {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    frame.render_widget(
        Paragraph::new(format!(" {status_text}")).style(status_style),
        chunks[1],
    );

    let msg_area = chunks[2];

    if app.show_repo_panel && msg_area.width > 35 {
        let h_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
            .split(msg_area);

        app.view_height.set(h_chunks[0].height as usize);
        draw_messages(frame, h_chunks[0], app);
        draw_sidebar(frame, h_chunks[1], app);
    } else {
        app.view_height.set(msg_area.height as usize);
        draw_messages(frame, msg_area, app);
    }

    draw_input(frame, chunks[3], app);

    if app.palette_open {
        draw_command_palette(frame, app);
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let header_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    let scrolled = if !app.auto_scroll.get() {
        let offset = app.scroll_offset.get();
        let total = app.total_lines.get();
        offset
            .checked_mul(100)
            .and_then(|v| v.checked_div(total))
            .map(|v| format!(" [{}%]", v.min(99)))
            .unwrap_or_default()
    } else {
        String::new()
    };
    let panel_indicator = if app.show_repo_panel {
        let tab_names = ["RepoMap", "Session", "MCPs"];
        format!(" [Tab:{}]", tab_names[app.sidebar_tab.min(2)])
    } else {
        String::new()
    };
    let spinner = if app.streaming {
        let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let idx = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            / 100) as usize
            % frames.len();
        format!("{}", frames[idx])
    } else {
        String::new()
    };
    let text = format!(
        "{} zuc1fer | {} | {}in {}out | {}{}{}",
        spinner, app.model, app.tokens_in, app.tokens_out, app.status, scrolled, panel_indicator
    );
    frame.render_widget(
        Paragraph::new(text).style(Style::default().bg(Color::DarkGray).fg(Color::White)),
        header_rows[0],
    );

    let total_tokens = app.tokens_in + app.tokens_out;
    let ratio = if app.context_max_tokens > 0 {
        ((total_tokens as f64 / app.context_max_tokens as f64) * 100.0).min(100.0) as u16
    } else {
        0
    };
    let gauge_label = format!(" context {}/{} ", total_tokens, app.context_max_tokens);
    let gauge = Gauge::default()
        .gauge_style(
            Style::default()
                .fg(if ratio > 80 {
                    Color::Red
                } else if ratio > 50 {
                    Color::Yellow
                } else {
                    Color::Green
                })
                .bg(Color::DarkGray),
        )
        .percent(ratio)
        .label(gauge_label);
    frame.render_widget(gauge, header_rows[1]);
}

fn palette_commands() -> [(&'static str, &'static str); 10] {
    [
        ("/model", "Switch AI model"),
        ("/models", "List available models"),
        ("/session", "Manage sessions"),
        ("/clear", "Clear current session"),
        ("/quit", "Exit zuc1fer"),
        ("/q", "Exit zuc1fer (short)"),
        ("/help", "Show help"),
        ("/config", "Show config path"),
        ("/toggle-sidebar", "Toggle sidebar panel"),
        ("/toggle-repo", "Toggle RepoMap sidebar"),
    ]
}

fn draw_command_palette(frame: &mut Frame, app: &App) {
    let area = centered_rect(60, 50, frame.area());
    frame.render_widget(Clear, area);

    let commands = palette_commands();

    let filtered: Vec<(&str, &str)> = if app.palette_query.is_empty() {
        commands.iter().copied().collect()
    } else {
        let q = app.palette_query.to_lowercase();
        commands
            .iter()
            .filter(|(cmd, desc)| {
                cmd.to_lowercase().contains(&q) || desc.to_lowercase().contains(&q)
            })
            .copied()
            .collect()
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let query_text = format!("> {}", app.palette_query);
    let input_block = Paragraph::new(query_text)
        .block(Block::bordered().title(" Command Palette "))
        .style(Style::default().fg(Color::White));
    frame.render_widget(input_block, chunks[0]);

    if !filtered.is_empty() {
        let items: Vec<String> = filtered
            .iter()
            .map(|(cmd, desc)| format!(" {:<20} {}", cmd, desc))
            .collect();

        let sel = app.palette_selection.min(filtered.len().saturating_sub(1));
        let mut state = ListState::default().with_selected(Some(sel));

        let list =
            List::new(items).highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan));

        frame.render_stateful_widget(list, chunks[1], &mut state);
    }

    if !app.streaming {
        frame.set_cursor_position((
            chunks[0].x + app.palette_query.len() as u16 + 2,
            chunks[0].y + 1,
        ));
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn draw_sidebar(frame: &mut Frame, area: Rect, app: &App) {
    let tab_titles = vec![" RepoMap ", " Session ", " MCPs "];
    let tabs = Tabs::new(tab_titles)
        .block(
            Block::default()
                .borders(Borders::LEFT | Borders::BOTTOM)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .select(app.sidebar_tab)
        .highlight_style(Style::default().fg(Color::Cyan))
        .divider("|");

    let tab_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    frame.render_widget(tabs, tab_rows[0]);

    let content_area = tab_rows[1];
    match app.sidebar_tab {
        0 => draw_repo_tab(frame, content_area, app),
        1 => draw_session_tab(frame, content_area, app),
        2 => draw_mcp_tab(frame, content_area, app),
        _ => {}
    }
}

fn draw_repo_tab(frame: &mut Frame, area: Rect, app: &App) {
    let mut lines: Vec<Line> = Vec::new();
    if app.repo_files.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            " (no data yet)",
            Style::default().fg(Color::DarkGray),
        )]));
    } else {
        let mut sorted: Vec<(String, f64)> = app.repo_files.clone();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));

        let tree = build_file_tree(&sorted);
        render_tree(&mut lines, &tree, "");
    }
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::LEFT)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );
}

fn build_file_tree(files: &[(String, f64)]) -> Vec<FileTreeNode> {
    let mut roots: Vec<FileTreeNode> = Vec::new();
    for (path, score) in files {
        let parts: Vec<&str> = path.split('/').collect();
        insert_into_tree(&mut roots, &parts, *score);
    }
    roots
}

#[derive(Clone)]
struct FileTreeNode {
    name: String,
    score: Option<f64>,
    children: Vec<FileTreeNode>,
}

fn insert_into_tree(nodes: &mut Vec<FileTreeNode>, parts: &[&str], score: f64) {
    if parts.is_empty() {
        return;
    }
    let name = parts[0].to_string();
    if parts.len() == 1 {
        nodes.push(FileTreeNode {
            name,
            score: Some(score),
            children: vec![],
        });
        return;
    }
    let existing = nodes.iter_mut().find(|n| n.name == name);
    if let Some(node) = existing {
        insert_into_tree(&mut node.children, &parts[1..], score);
    } else {
        let mut new_node = FileTreeNode {
            name: name.clone(),
            score: None,
            children: vec![],
        };
        insert_into_tree(&mut new_node.children, &parts[1..], score);
        nodes.push(new_node);
    }
}

fn render_tree(lines: &mut Vec<Line>, nodes: &[FileTreeNode], prefix: &str) {
    for (i, node) in nodes.iter().enumerate() {
        let is_node_last = i == nodes.len() - 1;
        let connector = if is_node_last {
            "└── "
        } else {
            "├── "
        };

        if let Some(score) = node.score {
            let color = if score > 0.5 {
                Color::White
            } else if score > 0.1 {
                Color::Gray
            } else {
                Color::DarkGray
            };
            let name = node.name.clone();
            let display_name = if name.len() > 24 {
                format!("{}...", &name[..21])
            } else {
                name
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{prefix}{connector}"),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(format!("{:.3}  ", score), Style::default().fg(color)),
                Span::styled(display_name, Style::default().fg(color)),
            ]));
        } else {
            lines.push(Line::from(vec![Span::styled(
                format!("{prefix}{connector}{}", node.name),
                Style::default().fg(Color::DarkGray),
            )]));
        }

        if !node.children.is_empty() {
            let new_prefix = format!("{}{}   ", prefix, if is_node_last { " " } else { "│" });
            render_tree(lines, &node.children, &new_prefix);
        }
    }
}

fn draw_session_tab(frame: &mut Frame, area: Rect, app: &App) {
    let total = app.tokens_in + app.tokens_out;
    let context_pct = if app.context_max_tokens > 0 {
        (total as f64 / app.context_max_tokens as f64) * 100.0
    } else {
        0.0
    };

    let lines = vec![
        Line::from(vec![Span::styled(
            " Session",
            Style::default().fg(Color::Cyan),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!(" Model: {}", app.model),
            Style::default().fg(Color::White),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!(" Tokens in:  {}", app.tokens_in),
            Style::default().fg(Color::Gray),
        )]),
        Line::from(vec![Span::styled(
            format!(" Tokens out: {}", app.tokens_out),
            Style::default().fg(Color::Gray),
        )]),
        Line::from(vec![Span::styled(
            format!(" Total:      {} ({:.0}%)", total, context_pct),
            Style::default().fg(Color::White),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!(" Est. cost:  ${:.4}", app.cost_usd),
            Style::default().fg(Color::Yellow),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            " Msgs: 0",
            Style::default().fg(Color::DarkGray),
        )]),
    ];

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::LEFT)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );
}

fn draw_mcp_tab(frame: &mut Frame, area: Rect, app: &App) {
    let mut lines = vec![
        Line::from(vec![Span::styled(
            " MCP Servers",
            Style::default().fg(Color::Cyan),
        )]),
        Line::from(""),
    ];

    if app.mcp_servers.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            " No MCP servers configured",
            Style::default().fg(Color::DarkGray),
        )]));
    } else {
        for (name, connected) in &app.mcp_servers {
            let (icon, color) = if *connected {
                (" ●", Color::Green)
            } else {
                (" ○", Color::Red)
            };
            lines.push(Line::from(vec![
                Span::styled(icon, Style::default().fg(color)),
                Span::styled(format!(" {name}"), Style::default().fg(Color::White)),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        " Configure in config.toml",
        Style::default().fg(Color::DarkGray),
    )]));

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::LEFT)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );
}

fn draw_messages(frame: &mut Frame, area: Rect, app: &App) {
    let msg_width = area.width.saturating_sub(2) as usize;
    let max_lines = area.height as usize;

    let all_lines = build_message_lines(&app.messages, msg_width);
    let total = all_lines.len();
    app.total_lines.set(total);

    let effective_scroll = if app.auto_scroll.get() {
        let new_scroll = total.saturating_sub(max_lines);
        app.scroll_offset.set(new_scroll);
        new_scroll
    } else {
        let clamped = app.scroll_offset.get().min(total.saturating_sub(max_lines));
        app.scroll_offset.set(clamped);
        clamped
    };

    let visible: Vec<Line> = all_lines
        .into_iter()
        .skip(effective_scroll)
        .take(max_lines)
        .collect();

    let mut lines = visible;
    while lines.len() < max_lines {
        lines.push(Line::from(""));
    }

    frame.render_widget(Paragraph::new(lines), area);

    if total > max_lines {
        let mut state = ScrollbarState::default()
            .content_length(total)
            .viewport_content_length(max_lines)
            .position(effective_scroll);

        let scrollbar_area = Rect {
            x: area.x + area.width.saturating_sub(1),
            y: area.y,
            width: 1,
            height: area.height,
        };
        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(Color::DarkGray)),
            scrollbar_area,
            &mut state,
        );
        app.scrollbar_state.set(state);
    }
}

fn build_message_lines(messages: &VecDeque<Message>, width: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    for msg in messages {
        match msg.role {
            MessageRole::User => {
                let wrapped = wrap_text(&msg.text, width.saturating_sub(2));
                for (i, w) in wrapped.iter().enumerate() {
                    if i == 0 {
                        lines.push(Line::from(vec![
                            Span::styled("> ", Style::default().fg(Color::Cyan)),
                            Span::styled(w.clone(), Style::default().fg(Color::White)),
                        ]));
                    } else {
                        lines.push(Line::from(vec![Span::styled(
                            format!("  {w}"),
                            Style::default().fg(Color::White),
                        )]));
                    }
                }
            }
            MessageRole::Assistant => {
                for w in wrap_text(&msg.text, width) {
                    if w.is_empty() {
                        lines.push(Line::from(""));
                    } else {
                        lines.push(Line::from(vec![Span::styled(
                            w,
                            Style::default().fg(Color::Gray),
                        )]));
                    }
                }
            }
            MessageRole::System => {
                for w in wrap_text(&format!("  {}", msg.text), width) {
                    lines.push(Line::from(vec![Span::styled(
                        w,
                        Style::default().fg(Color::DarkGray),
                    )]));
                }
            }
        }
    }
    lines
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width <= 2 {
        return text.lines().map(|s| s.to_string()).collect();
    }
    let mut result = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            result.push(String::new());
        } else {
            wrap_line(line, width, &mut result);
        }
    }
    result
}

fn wrap_line(line: &str, width: usize, result: &mut Vec<String>) {
    let mut remaining = line;
    while !remaining.is_empty() {
        if remaining.len() <= width {
            result.push(remaining.to_string());
            break;
        }
        let boundary = width.min(remaining.len());
        let mut split_at = boundary;
        if let Some(space_idx) = remaining[..boundary].rfind(' ') {
            if space_idx > 0 {
                split_at = space_idx;
            }
        }
        result.push(remaining[..split_at].to_string());
        remaining = remaining[split_at..].trim_start();
    }
}

fn draw_input(frame: &mut Frame, area: Rect, app: &App) {
    let mut input = app.input.clone();
    input.set_style(if app.streaming {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    });
    input.set_block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(&input, area);

    if !app.streaming {
        frame.set_cursor_position((area.x + app.input.cursor().0 as u16 + 1, area.y));
    }
}

fn model_pricing(provider: &str) -> (f64, f64) {
    match provider {
        "deepseek" => (0.55, 2.19),
        "anthropic" => (3.0, 15.0),
        "openai" => (2.50, 10.0),
        "openrouter" => (0.50, 1.50),
        "ollama" => (0.0, 0.0),
        _ => (1.0, 5.0),
    }
}
