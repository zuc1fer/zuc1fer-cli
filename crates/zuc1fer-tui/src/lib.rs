use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, Gauge, List, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Tabs,
    },
    Frame,
};
use std::cell::Cell;
use std::collections::VecDeque;
use std::sync::OnceLock;
use tui_textarea::TextArea;

static SYNTAX_SET: OnceLock<syntect::parsing::SyntaxSet> = OnceLock::new();
static THEME: OnceLock<syntect::highlighting::Theme> = OnceLock::new();

fn get_syntax_set() -> &'static syntect::parsing::SyntaxSet {
    SYNTAX_SET.get_or_init(syntect::parsing::SyntaxSet::load_defaults_newlines)
}

fn get_theme() -> &'static syntect::highlighting::Theme {
    THEME.get_or_init(|| {
        let ts = syntect::highlighting::ThemeSet::load_defaults();
        ts.themes["base16-ocean.dark"].clone()
    })
}

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
    pub repo_scroll: usize,
    pub available_models: Vec<String>,
    pub show_model_picker: bool,
    pub model_picker_query: String,
    pub model_picker_selection: usize,
    pub sessions: Vec<SessionInfo>,
    pub show_session_picker: bool,
    pub session_picker_selection: usize,
    pub todos: Vec<TodoItem>,
    pub pending_approval: Option<(String, String)>,
    pub input_history: Vec<String>,
    history_index: Option<usize>,
    saved_input: Option<String>,
    last_assistant_idx: usize,
    scroll_offset: Cell<usize>,
    auto_scroll: Cell<bool>,
    total_lines: Cell<usize>,
    view_height: Cell<usize>,
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

#[derive(Clone)]
pub struct TodoItem {
    pub text: String,
    pub done: bool,
}

#[derive(Clone)]
pub struct SessionInfo {
    pub id: String,
    pub model: String,
    pub message_count: usize,
    pub total_tokens: u64,
    pub updated_at: String,
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
            context_max_tokens: model_context_limit(model),
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
            repo_scroll: 0,
            available_models: Vec::new(),
            show_model_picker: false,
            model_picker_query: String::new(),
            model_picker_selection: 0,
            sessions: Vec::new(),
            show_session_picker: false,
            session_picker_selection: 0,
            todos: Vec::new(),
            pending_approval: None,
            input_history: Vec::new(),
            history_index: None,
            saved_input: None,
            last_assistant_idx: 0,
            scroll_offset: Cell::new(0),
            auto_scroll: Cell::new(true),
            total_lines: Cell::new(0),
            view_height: Cell::new(24),
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
        self.parse_todos();
    }

    fn parse_todos(&mut self) {
        let mut items: Vec<TodoItem> = Vec::new();
        for msg in &self.messages {
            for line in msg.text.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed
                    .strip_prefix("- [x] ")
                    .or_else(|| trimmed.strip_prefix("- [X] "))
                {
                    items.push(TodoItem {
                        text: rest.to_string(),
                        done: true,
                    });
                } else if let Some(rest) = trimmed.strip_prefix("- [ ] ") {
                    items.push(TodoItem {
                        text: rest.to_string(),
                        done: false,
                    });
                } else if let Some(rest) = trimmed
                    .strip_prefix("[x] ")
                    .or_else(|| trimmed.strip_prefix("[X] "))
                {
                    items.push(TodoItem {
                        text: rest.to_string(),
                        done: true,
                    });
                } else if let Some(rest) = trimmed.strip_prefix("[ ] ") {
                    items.push(TodoItem {
                        text: rest.to_string(),
                        done: false,
                    });
                } else if trimmed.starts_with("✅") || trimmed.starts_with("- ✅") {
                    let text = trimmed
                        .trim_start_matches("- ")
                        .trim_start_matches("✅ ")
                        .to_string();
                    items.push(TodoItem { text, done: true });
                }
            }
        }
        self.todos = items;
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
        if !text.is_empty() && self.input_history.last().map(|s| s.as_str()) != Some(&text) {
            self.input_history.push(text.clone());
        }
        self.history_index = None;
        self.saved_input = None;
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

    fn navigate_history_back(&mut self) {
        if self.input_history.is_empty() {
            return;
        }
        if self.history_index.is_none() {
            self.saved_input = Some(self.input.lines().join("\n"));
            self.history_index = Some(self.input_history.len() - 1);
        } else if let Some(idx) = self.history_index {
            if idx > 0 {
                self.history_index = Some(idx - 1);
            }
        }
        if let Some(idx) = self.history_index {
            if let Some(entry) = self.input_history.get(idx) {
                self.input = TextArea::from([entry.clone()]);
                self.input.set_style(Style::default().fg(Color::White));
                self.input.set_cursor_line_style(Style::default());
            }
        }
    }

    fn navigate_history_forward(&mut self) {
        if let Some(idx) = self.history_index {
            if idx + 1 < self.input_history.len() {
                self.history_index = Some(idx + 1);
                if let Some(entry) = self.input_history.get(idx + 1) {
                    self.input = TextArea::from([entry.clone()]);
                    self.input.move_cursor(tui_textarea::CursorMove::End);
                    self.input.set_style(Style::default().fg(Color::White));
                    self.input.set_cursor_line_style(Style::default());
                }
            } else {
                self.history_index = None;
                let saved = self.saved_input.take().unwrap_or_default();
                self.input = TextArea::from([saved]);
                self.input.set_style(Style::default().fg(Color::White));
                self.input.set_cursor_line_style(Style::default());
            }
        }
    }

    pub fn update_cost(&mut self) {
        let (price_in, price_out) = model_pricing(&self.model);
        let in_cost = (self.tokens_in as f64 / 1_000_000.0) * price_in;
        let out_cost = (self.tokens_out as f64 / 1_000_000.0) * price_out;
        self.cost_usd = in_cost + out_cost;
        self.context_max_tokens = model_context_limit(&self.model);
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
        if self.show_model_picker {
            match key.code {
                KeyCode::Esc => {
                    self.show_model_picker = false;
                    self.model_picker_query.clear();
                    self.model_picker_selection = 0;
                }
                KeyCode::Enter => {
                    let sel = self.model_picker_selection;
                    let chosen = filtered_models(&self.available_models, &self.model_picker_query)
                        .get(sel)
                        .map(|m| (*m).clone());
                    self.show_model_picker = false;
                    self.model_picker_query.clear();
                    self.model_picker_selection = 0;
                    if let Some(model) = chosen {
                        return Some(format!("__MODEL_SELECT__:{}", model));
                    }
                }
                KeyCode::Up => {
                    self.model_picker_selection = self.model_picker_selection.saturating_sub(1);
                }
                KeyCode::Down => {
                    self.model_picker_selection += 1;
                }
                KeyCode::Backspace => {
                    self.model_picker_query.pop();
                    self.model_picker_selection = 0;
                }
                KeyCode::Char(c) => {
                    self.model_picker_query.push(c);
                    self.model_picker_selection = 0;
                }
                _ => {}
            }
            return None;
        }

        if self.show_session_picker {
            match key.code {
                KeyCode::Esc => {
                    self.show_session_picker = false;
                    self.session_picker_selection = 0;
                }
                KeyCode::Enter => {
                    let sel = self.session_picker_selection;
                    self.show_session_picker = false;
                    self.session_picker_selection = 0;
                    if let Some(s) = self.sessions.get(sel) {
                        return Some(format!("__SESSION_SELECT__:{}", s.id));
                    }
                }
                KeyCode::Up => {
                    self.session_picker_selection = self.session_picker_selection.saturating_sub(1);
                }
                KeyCode::Down => {
                    self.session_picker_selection += 1;
                }
                _ => {}
            }
            return None;
        }

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
                    self.sidebar_tab = (self.sidebar_tab + 1) % 4;
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
                if self.show_repo_panel && self.sidebar_tab == 0 {
                    self.repo_scroll = self.repo_scroll.saturating_sub(10);
                } else {
                    let page = self.view_height.get().saturating_sub(2);
                    self.scroll_offset
                        .set(self.scroll_offset.get().saturating_sub(page));
                    self.auto_scroll.set(false);
                }
            }
            KeyCode::PageDown => {
                if self.show_repo_panel && self.sidebar_tab == 0 {
                    self.repo_scroll += 10;
                } else {
                    let page = self.view_height.get().saturating_sub(2);
                    let max = self.max_scroll();
                    let new = (self.scroll_offset.get() + page).min(max);
                    self.scroll_offset.set(new);
                    if new >= max {
                        self.auto_scroll.set(true);
                    }
                }
            }
            KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.messages.clear();
                self.add_system_message("Screen cleared.".into());
            }
            KeyCode::Up => {
                if self.show_repo_panel && self.sidebar_tab == 0 {
                    self.repo_scroll = self.repo_scroll.saturating_sub(1);
                } else if self.input.lines().len() <= 1 && self.input.cursor().1 == 0 {
                    self.navigate_history_back();
                } else {
                    self.scroll_offset
                        .set(self.scroll_offset.get().saturating_sub(1));
                    self.auto_scroll.set(false);
                }
            }
            KeyCode::Down => {
                if self.show_repo_panel && self.sidebar_tab == 0 {
                    self.repo_scroll += 1;
                } else if self.history_index.is_some() {
                    self.navigate_history_forward();
                } else {
                    let max = self.max_scroll();
                    let new = (self.scroll_offset.get() + 1).min(max);
                    self.scroll_offset.set(new);
                    if new >= max {
                        self.auto_scroll.set(true);
                    }
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
        " ● Streaming...  (Esc to cancel)"
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
    if app.show_model_picker {
        draw_model_picker(frame, app);
    }
    if app.show_session_picker {
        draw_session_picker(frame, app);
    }
    if app.pending_approval.is_some() {
        draw_approval_modal(frame, app);
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
        let tab_names = ["RepoMap", "Session", "MCPs", "Todos"];
        format!(" [Tab:{}]", tab_names[app.sidebar_tab.min(3)])
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
        frames[idx].to_string()
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
        commands.to_vec()
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

fn draw_model_picker(frame: &mut Frame, app: &App) {
    let area = centered_rect(60, 60, frame.area());
    frame.render_widget(Clear, area);

    let models = filtered_models(&app.available_models, &app.model_picker_query);
    let items: Vec<String> = models
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let marker: String = if m.as_str() == app.model.as_str() {
                " ●".into()
            } else {
                format!(" {}.", i + 1)
            };
            format!("{} {}", marker, m)
        })
        .collect();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let query_text = format!("> {}", app.model_picker_query);
    let nb = format!("{} models", app.available_models.len());
    frame.render_widget(
        Paragraph::new(query_text).block(Block::bordered().title(format!(" Switch Model ({nb}) "))),
        chunks[0],
    );

    if !items.is_empty() {
        let sel = app
            .model_picker_selection
            .min(items.len().saturating_sub(1));
        let mut state = ListState::default().with_selected(Some(sel));
        frame.render_stateful_widget(
            List::new(items).highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan)),
            chunks[1],
            &mut state,
        );
    }

    if !app.streaming {
        frame.set_cursor_position((
            chunks[0].x + app.model_picker_query.len() as u16 + 2,
            chunks[0].y + 1,
        ));
    }
}

fn filtered_models<'a>(models: &'a [String], query: &str) -> Vec<&'a String> {
    if query.is_empty() {
        return models.iter().collect();
    }
    let q = query.to_lowercase();
    models
        .iter()
        .filter(|m| m.to_lowercase().contains(&q))
        .collect()
}

fn draw_session_picker(frame: &mut Frame, app: &App) {
    let area = centered_rect(70, 60, frame.area());
    frame.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let count = app.sessions.len();
    frame.render_widget(
        Paragraph::new(" Use ↑↓ to navigate, Enter to select, Esc to close ")
            .block(Block::bordered().title(format!(" Sessions ({count}) "))),
        chunks[0],
    );

    if app.sessions.is_empty() {
        let msg = Paragraph::new(" No saved sessions.\n\n Start a chat to auto-save.")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(msg, chunks[1]);
    } else {
        let items: Vec<String> = app
            .sessions
            .iter()
            .map(|s| {
                format!(
                    " {} | {}msgs | {}tk | {} | {}",
                    s.model,
                    s.message_count,
                    s.total_tokens,
                    &s.updated_at[..s.updated_at.len().min(16)],
                    &s.id[..s.id.len().min(8)],
                )
            })
            .collect();

        let sel = app
            .session_picker_selection
            .min(items.len().saturating_sub(1));
        let mut state = ListState::default().with_selected(Some(sel));
        frame.render_stateful_widget(
            List::new(items).highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan)),
            chunks[1],
            &mut state,
        );
    }
}

fn draw_approval_modal(frame: &mut Frame, app: &App) {
    let (tool, detail) = match &app.pending_approval {
        Some(p) => p,
        None => return,
    };
    let area = centered_rect(60, 30, frame.area());
    frame.render_widget(Clear, area);

    let detail_disp: String = detail.chars().take(200).collect();
    let lines = vec![
        Line::from(vec![Span::styled(
            format!(" Tool: {tool}"),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!("  {detail_disp}"),
            Style::default().fg(Color::White),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  [y] approve    [n] deny    [a] approve all this session",
            Style::default().fg(Color::Gray),
        )]),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::bordered().title(" Approval required "))
            .style(Style::default().fg(Color::White)),
        area,
    );
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
    let tab_titles = vec![" RepoMap ", " Session ", " MCPs ", " Todos "];
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
        3 => draw_todos_tab(frame, content_area, app),
        _ => {}
    }
}

fn draw_repo_tab(frame: &mut Frame, area: Rect, app: &App) {
    let max_lines = area.height.saturating_sub(1) as usize;
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
    let total = lines.len();
    let scroll = app.repo_scroll.min(total.saturating_sub(max_lines.max(1)));
    let visible: Vec<Line> = lines.into_iter().skip(scroll).take(max_lines).collect();

    frame.render_widget(
        Paragraph::new(visible).block(
            Block::default()
                .borders(Borders::LEFT)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );

    if total > max_lines {
        let state = ScrollbarState::default()
            .content_length(total.max(1))
            .viewport_content_length(max_lines.max(1))
            .position(scroll);
        let sb_area = Rect {
            x: area.x + area.width.saturating_sub(1),
            y: area.y,
            width: 1,
            height: area.height,
        };
        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(Color::DarkGray)),
            sb_area,
            &mut state.clone(),
        );
    }
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
            let display_name = if name.chars().count() > 24 {
                let prefix: String = name.chars().take(21).collect();
                format!("{prefix}...")
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
            format!(" Msgs: {}", app.messages.len()),
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

fn draw_todos_tab(frame: &mut Frame, area: Rect, app: &App) {
    let done_count = app.todos.iter().filter(|t| t.done).count();
    let total = app.todos.len();
    let pct = (done_count * 100).checked_div(total).unwrap_or(0);

    let mut lines = vec![
        Line::from(vec![Span::styled(
            format!(" Progress: {}/{} ({}%)", done_count, total, pct),
            Style::default().fg(Color::Cyan),
        )]),
        Line::from(""),
    ];

    if app.todos.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            " No todos found in conversation.",
            Style::default().fg(Color::DarkGray),
        )]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            " Use checkboxes in prompts:",
            Style::default().fg(Color::DarkGray),
        )]));
        lines.push(Line::from(vec![Span::styled(
            " - [ ] Task name",
            Style::default().fg(Color::DarkGray),
        )]));
    } else {
        for item in &app.todos {
            let (icon, color) = if item.done {
                (" [x] ", Color::Green)
            } else {
                (" [ ] ", Color::Gray)
            };
            lines.push(Line::from(vec![
                Span::styled(icon, Style::default().fg(color)),
                Span::styled(&item.text, Style::default().fg(color)),
            ]));
        }
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

    let state = ScrollbarState::default()
        .content_length(total.max(1))
        .viewport_content_length(max_lines.max(1))
        .position(effective_scroll.min(total.saturating_sub(max_lines.max(1))));

    let scrollbar_area = Rect {
        x: area.x + area.width.saturating_sub(1),
        y: area.y,
        width: 1,
        height: area.height,
    };
    let mut s = state;
    frame.render_stateful_widget(
        Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(Color::DarkGray)),
        scrollbar_area,
        &mut s,
    );
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
                let mut in_code_block = false;
                let mut code_lang = String::new();
                let mut code_buffer: Vec<String> = Vec::new();
                for text_line in msg.text.lines() {
                    let trimmed = text_line.trim();
                    if trimmed.starts_with("```") {
                        if in_code_block {
                            if !code_buffer.is_empty() {
                                for hl in highlight_code(&code_buffer, &code_lang) {
                                    lines.push(hl);
                                }
                            }
                            code_buffer.clear();
                            in_code_block = false;
                            lines.push(Line::from(vec![Span::styled(
                                text_line.to_string(),
                                Style::default().fg(Color::DarkGray),
                            )]));
                        } else {
                            in_code_block = true;
                            code_lang =
                                trimmed.strip_prefix("```").unwrap_or("").trim().to_string();
                            lines.push(Line::from(vec![Span::styled(
                                text_line.to_string(),
                                Style::default().fg(Color::DarkGray),
                            )]));
                        }
                        continue;
                    }
                    if in_code_block {
                        code_buffer.push(text_line.to_string());
                        continue;
                    }
                    let trimmed_para = text_line.trim_start();
                    let is_block = trimmed_para.starts_with('#')
                        || trimmed_para.starts_with("- ")
                        || trimmed_para.starts_with("* ")
                        || trimmed_para.starts_with("> ")
                        || trimmed_para == "---"
                        || trimmed_para == "***"
                        || trimmed_para == "___"
                        || numbered_prefix(trimmed_para).is_some();
                    if is_block {
                        lines.push(Line::from(render_markdown_line(text_line)));
                    } else {
                        for w in wrap_text(text_line, width) {
                            if w.is_empty() {
                                lines.push(Line::from(""));
                            } else {
                                lines.push(Line::from(render_inline_markdown(&w)));
                            }
                        }
                    }
                }
            }
            MessageRole::System => {
                let is_error = msg.text.contains("Error") || msg.text.contains("error");
                let color = if is_error {
                    Color::Red
                } else {
                    Color::DarkGray
                };
                for w in wrap_text(&format!("  {}", msg.text), width) {
                    lines.push(Line::from(vec![Span::styled(
                        w,
                        Style::default().fg(color),
                    )]));
                }
            }
        }
    }
    lines
}

fn render_markdown_line(line: &str) -> Vec<Span<'static>> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return vec![Span::raw("")];
    }
    if trimmed.starts_with("### ") {
        return vec![Span::styled(
            trimmed.strip_prefix("### ").unwrap_or(trimmed).to_string(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )];
    }
    if trimmed.starts_with("## ") {
        return vec![Span::styled(
            trimmed.strip_prefix("## ").unwrap_or(trimmed).to_string(),
            Style::default().fg(Color::Cyan),
        )];
    }
    if trimmed.starts_with("# ") {
        return vec![Span::styled(
            trimmed.strip_prefix("# ").unwrap_or(trimmed).to_string(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )];
    }
    if trimmed.starts_with("> ") {
        let content = trimmed.strip_prefix("> ").unwrap_or(trimmed);
        return vec![Span::styled(
            format!("│ {content}"),
            Style::default().fg(Color::DarkGray),
        )];
    }
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        let content = &trimmed[2..];
        return vec![Span::styled(
            format!("  • {content}"),
            Style::default().fg(Color::Gray),
        )];
    }
    if let Some(cap) = numbered_prefix(trimmed) {
        return vec![Span::styled(cap, Style::default().fg(Color::Gray))];
    }
    if trimmed == "---" || trimmed == "***" || trimmed == "___" {
        let bar = "─".repeat(40);
        return vec![Span::styled(bar, Style::default().fg(Color::DarkGray))];
    }
    render_inline_markdown(line)
}

fn numbered_prefix(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i > 0 && i < bytes.len() && bytes[i] == b'.' && i + 1 < bytes.len() && bytes[i + 1] == b' ' {
        Some(format!("  {}. {}", &s[..i], s[i + 2..].trim()))
    } else {
        None
    }
}

fn render_inline_markdown(line: &str) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut remaining = line;
    let base = Style::default().fg(Color::Gray);

    while !remaining.is_empty() {
        if let Some(start) = remaining.find("**") {
            if start > 0 {
                spans.push(Span::styled(remaining[..start].to_string(), base));
            }
            remaining = &remaining[start + 2..];
            if let Some(end) = remaining.find("**") {
                spans.push(Span::styled(
                    remaining[..end].to_string(),
                    base.add_modifier(Modifier::BOLD),
                ));
                remaining = &remaining[end + 2..];
            } else {
                spans.push(Span::styled(format!("**{remaining}"), base));
                remaining = "";
            }
        } else if let Some(start) = remaining.find('`') {
            if start > 0 {
                spans.push(Span::styled(remaining[..start].to_string(), base));
            }
            remaining = &remaining[start + 1..];
            if let Some(end) = remaining.find('`') {
                spans.push(Span::styled(
                    remaining[..end].to_string(),
                    Style::default().fg(Color::Yellow).bg(Color::DarkGray),
                ));
                remaining = &remaining[end + 1..];
            } else {
                spans.push(Span::styled(format!("`{remaining}"), base));
                remaining = "";
            }
        } else {
            spans.push(Span::styled(remaining.to_string(), base));
            break;
        }
    }
    spans
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
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= width {
        result.push(line.to_string());
        return;
    }
    let mut start = 0;
    while start < chars.len() {
        let end = (start + width).min(chars.len());
        let mut split = end;
        if end < chars.len() {
            if let Some(rel) = chars[start..end].iter().rposition(|&c| c == ' ') {
                if rel > 0 {
                    split = start + rel;
                }
            }
        }
        let segment: String = chars[start..split].iter().collect();
        result.push(segment.trim_end().to_string());
        start = split;
        while start < chars.len() && chars[start] == ' ' {
            start += 1;
        }
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
        frame.set_cursor_position((area.x + app.input.cursor().1 as u16 + 1, area.y));
    }
}

fn model_pricing(model: &str) -> (f64, f64) {
    let (provider, name) = model.split_once('/').unwrap_or(("", model));
    let m = name.to_lowercase();
    match provider {
        "deepseek" => {
            if m.contains("pro") {
                (0.435, 0.87)
            } else {
                (0.14, 0.28)
            }
        }
        "anthropic" => {
            if m.contains("opus") {
                (15.0, 75.0)
            } else if m.contains("haiku") {
                (0.80, 4.0)
            } else {
                (3.0, 15.0)
            }
        }
        "openai" => {
            if m.contains("nano") {
                (0.10, 0.40)
            } else if m.contains("mini") {
                (0.15, 0.60)
            } else if m.contains("4.1") {
                (2.0, 8.0)
            } else {
                (2.50, 10.0)
            }
        }
        "openrouter" => (0.50, 1.50),
        "ollama" => (0.0, 0.0),
        _ => (1.0, 5.0),
    }
}

fn model_context_limit(model: &str) -> u64 {
    let provider = model.split('/').next().unwrap_or("");
    let model_lower = model.to_lowercase();
    match provider {
        "deepseek" => 1_048_576,
        "anthropic" => 200_000,
        "openai" => {
            if model_lower.contains("gpt-4.1") {
                1_047_576
            } else {
                128_000
            }
        }
        "openrouter" => 131_072,
        "ollama" => 8_192,
        _ => 128_000,
    }
}

fn highlight_code(lines: &[String], language: &str) -> Vec<Line<'static>> {
    let ss = get_syntax_set();
    let syntax = ss
        .find_syntax_by_token(language)
        .or_else(|| ss.find_syntax_by_extension(language))
        .unwrap_or_else(|| ss.find_syntax_plain_text());

    let theme = get_theme();
    let mut highlighter = syntect::easy::HighlightLines::new(syntax, theme);
    let mut result = Vec::new();

    for line in lines {
        let mut spans: Vec<Span<'static>> = vec![Span::raw("  ")];
        match highlighter.highlight_line(line, ss) {
            Ok(ranges) => {
                for (style, text) in ranges {
                    let fg = style.foreground;
                    spans.push(Span::styled(
                        text.to_string(),
                        Style::default().fg(Color::Rgb(fg.r, fg.g, fg.b)),
                    ));
                }
            }
            Err(_) => spans.push(Span::styled(line.clone(), Style::default().fg(Color::Gray))),
        }
        result.push(Line::from(spans));
    }
    result
}
