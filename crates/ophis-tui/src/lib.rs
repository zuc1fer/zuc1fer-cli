use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
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

mod ascii_art;
mod theme;
use theme::*;

static SYNTAX_SET: OnceLock<syntect::parsing::SyntaxSet> = OnceLock::new();
static THEME: OnceLock<syntect::highlighting::Theme> = OnceLock::new();

fn get_syntax_set() -> &'static syntect::parsing::SyntaxSet {
    SYNTAX_SET.get_or_init(syntect::parsing::SyntaxSet::load_defaults_newlines)
}

fn get_theme() -> &'static syntect::highlighting::Theme {
    THEME.get_or_init(|| {
        let ts = syntect::highlighting::ThemeSet::load_defaults();
        ts.themes
            .get("dracula")
            .or_else(|| ts.themes.get("base16-mocha.dark"))
            .cloned()
            .unwrap_or_else(|| ts.themes["base16-ocean.dark"].clone())
    })
}

pub struct App {
    pub messages: VecDeque<Message>,
    pub input: TextArea<'static>,
    pub status: String,
    pub model: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub active_ctx_in: u64,
    pub active_ctx_out: u64,
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
            active_ctx_in: 0,
            active_ctx_out: 0,
            context_max_tokens: model_context_limit(model),
            running: true,
            streaming: false,
            repo_files: Vec::new(),
            show_repo_panel: true,
            sidebar_tab: 1,
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

    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(4),
        Constraint::Length(3),
    ])
    .split(area);

    draw_header(frame, chunks[0], app);

    let body = chunks[1];

    if app.messages.is_empty() && !app.streaming {
        app.view_height.set(body.height as usize);
        draw_splash(frame, body);
    } else if app.show_repo_panel && body.width > 70 {
        let h_chunks =
            Layout::horizontal([Constraint::Min(20), Constraint::Length(48)]).split(body);

        app.view_height.set(h_chunks[0].height as usize);
        draw_messages(frame, h_chunks[0], app);
        draw_sidebar(frame, h_chunks[1], app);
    } else {
        app.view_height.set(body.height as usize);
        draw_messages(frame, body, app);
    }

    draw_input(frame, chunks[2], app);

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
    let (dot_color, status_label): (Color, String) = if app.streaming {
        (WARN, "streaming  ·  Esc to cancel".to_string())
    } else {
        (ACCENT, app.status.clone())
    };

    let sep = Span::styled("  ·  ", Style::default().fg(ACCENT_DIM));
    let mut spans: Vec<Span> = Vec::new();
    if !spinner.is_empty() {
        spans.push(Span::styled(
            format!("{spinner} "),
            Style::default().fg(WARN),
        ));
    } else {
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled("◆ ", Style::default().fg(ACCENT)));
    spans.push(Span::styled("ophis", accent_bold()));
    spans.push(sep.clone());
    spans.push(Span::styled(
        app.model.clone(),
        Style::default().fg(TEXT_DIM),
    ));
    spans.push(sep.clone());
    spans.push(Span::styled("●", Style::default().fg(dot_color)));
    spans.push(Span::styled(
        format!(" {status_label}"),
        Style::default().fg(TEXT_DIM),
    ));
    spans.push(sep.clone());
    spans.push(Span::styled(
        format!("{}↑ {}↓", app.tokens_in, app.tokens_out),
        Style::default().fg(TEXT_DIM),
    ));
    if !scrolled.is_empty() {
        spans.push(Span::styled(scrolled, Style::default().fg(ACCENT_DIM)));
    }
    if !panel_indicator.is_empty() {
        spans.push(Span::styled(
            panel_indicator,
            Style::default().fg(ACCENT_DIM),
        ));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(SURFACE)),
        header_rows[0],
    );

    let active_tokens = app.active_ctx_in + app.active_ctx_out;
    let ratio = if app.context_max_tokens > 0 {
        ((active_tokens as f64 / app.context_max_tokens as f64) * 100.0).min(100.0) as u16
    } else {
        0
    };
    let fill = if ratio > 85 { ERROR } else { ACCENT };
    let gauge_label = format!(" {active_tokens} / {} ctx ", app.context_max_tokens);
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(fill).bg(SURFACE_LIGHT))
        .percent(ratio)
        .label(Span::styled(gauge_label, Style::default().fg(TEXT)));
    frame.render_widget(gauge, header_rows[1]);
}

fn palette_commands() -> [(&'static str, &'static str); 10] {
    [
        ("/model", "Switch AI model"),
        ("/models", "List available models"),
        ("/session", "Manage sessions"),
        ("/clear", "Clear current session"),
        ("/quit", "Exit ophis"),
        ("/q", "Exit ophis (short)"),
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

    let block = modal_block("Command Palette");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("› ", accent()),
            Span::styled(app.palette_query.clone(), text()),
        ])),
        chunks[0],
    );

    if !filtered.is_empty() {
        let items: Vec<Line> = filtered
            .iter()
            .map(|(cmd, desc)| {
                Line::from(vec![
                    Span::styled(format!(" {cmd:<18}"), accent()),
                    Span::styled(format!(" {desc}"), dim()),
                ])
            })
            .collect();

        let sel = app.palette_selection.min(filtered.len().saturating_sub(1));
        let mut state = ListState::default().with_selected(Some(sel));
        let list = List::new(items)
            .highlight_style(selection())
            .highlight_symbol("▶ ");
        frame.render_stateful_widget(list, chunks[1], &mut state);
    }

    if !app.streaming {
        frame.set_cursor_position((
            chunks[0].x + 2 + app.palette_query.chars().count() as u16,
            chunks[0].y,
        ));
    }
}

fn draw_model_picker(frame: &mut Frame, app: &App) {
    let area = centered_rect(60, 60, frame.area());
    frame.render_widget(Clear, area);

    let models = filtered_models(&app.available_models, &app.model_picker_query);
    let items: Vec<Line> = models
        .iter()
        .map(|m| {
            let current = m.as_str() == app.model.as_str();
            let (marker, mcolor) = if current {
                (" ● ", ACCENT_LIGHT)
            } else {
                (" ◇ ", ACCENT_DIM)
            };
            let name_style = if current { accent_bold() } else { text() };
            Line::from(vec![
                Span::styled(marker, Style::default().fg(mcolor)),
                Span::styled((*m).clone(), name_style),
            ])
        })
        .collect();

    let nb = app.available_models.len();
    let block = modal_block(&format!("Switch Model · {nb}"));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(inner);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("› ", accent()),
            Span::styled(app.model_picker_query.clone(), text()),
        ])),
        chunks[0],
    );

    if !items.is_empty() {
        let sel = app
            .model_picker_selection
            .min(items.len().saturating_sub(1));
        let mut state = ListState::default().with_selected(Some(sel));
        frame.render_stateful_widget(
            List::new(items)
                .highlight_style(selection())
                .highlight_symbol("▶ "),
            chunks[1],
            &mut state,
        );
    }

    if !app.streaming {
        frame.set_cursor_position((
            chunks[0].x + 2 + app.model_picker_query.chars().count() as u16,
            chunks[0].y,
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

    let count = app.sessions.len();
    let block = modal_block(&format!("Sessions · {count}"));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(inner);
    frame.render_widget(
        Paragraph::new(Span::styled(
            " ↑↓ navigate · Enter select · Esc close",
            dim(),
        )),
        chunks[0],
    );

    if app.sessions.is_empty() {
        frame.render_widget(
            Paragraph::new("\n No saved sessions — start a chat to auto-save.").style(dim()),
            chunks[1],
        );
    } else {
        let items: Vec<Line> = app
            .sessions
            .iter()
            .map(|s| {
                Line::from(vec![
                    Span::styled(format!(" {}", s.model), accent()),
                    Span::styled(
                        format!(
                            "  {}msg · {}tk · {} · {}",
                            s.message_count,
                            s.total_tokens,
                            &s.updated_at[..s.updated_at.len().min(16)],
                            &s.id[..s.id.len().min(8)],
                        ),
                        dim(),
                    ),
                ])
            })
            .collect();

        let sel = app
            .session_picker_selection
            .min(items.len().saturating_sub(1));
        let mut state = ListState::default().with_selected(Some(sel));
        frame.render_stateful_widget(
            List::new(items)
                .highlight_style(selection())
                .highlight_symbol("▶ "),
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
    let area = centered_rect(60, 34, frame.area());
    frame.render_widget(Clear, area);

    let detail_disp: String = detail.chars().take(240).collect();
    let block = modal_block("Approval required");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  tool   ", dim()),
            Span::styled(tool.clone(), accent_bold()),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(format!("  {detail_disp}"), text())]),
        Line::from(""),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  [y]",
                Style::default().fg(SUCCESS).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" approve    ", text()),
            Span::styled(
                "[n]",
                Style::default().fg(ERROR).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" deny    ", text()),
            Span::styled("[a]", accent_bold()),
            Span::styled(" approve all session", text()),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
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
                .borders(Borders::LEFT)
                .border_style(Style::default().fg(ACCENT_DIM)),
        )
        .select(app.sidebar_tab)
        .style(Style::default().fg(TEXT_DIM))
        .highlight_style(accent_bold())
        .divider(Span::styled("·", Style::default().fg(ACCENT_DIM)));

    // Responsively show small Ouroboros logo at the top of the sidebar if there's enough height
    let (logo_area, tab_area, content_area) = if area.height >= 20 {
        let sidebar_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8), // Small ouroboros logo
                Constraint::Length(2), // Tabs selection widget
                Constraint::Min(0),    // Active tab content area
            ])
            .split(area);
        (Some(sidebar_layout[0]), sidebar_layout[1], sidebar_layout[2])
    } else {
        let sidebar_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Tabs selection widget
                Constraint::Min(0),    // Active tab content area
            ])
            .split(area);
        (None, sidebar_layout[0], sidebar_layout[1])
    };

    if let Some(logo_rect) = logo_area {
        let logo_height = 8;
        let logo_width = 16;
        let logo_lines = ascii_art::render_ouroboros(logo_width, logo_height);
        
        let padding_left = (logo_rect.width.saturating_sub(logo_width as u16)) / 2;
        let padding_str = " ".repeat(padding_left as usize);
        let centered_logo: Vec<Line> = logo_lines
            .into_iter()
            .map(|line| {
                let mut spans = vec![Span::raw(padding_str.clone())];
                spans.extend(line.spans);
                Line::from(spans)
            })
            .collect();
            
        frame.render_widget(Paragraph::new(centered_logo).block(panel_block()), logo_rect);
    }

    frame.render_widget(tabs, tab_area);

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
            Style::default().fg(TEXT_DIM),
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

    frame.render_widget(Paragraph::new(visible).block(panel_block()), area);

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
                .style(Style::default().fg(ACCENT_DIM)),
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
                ACCENT_LIGHT
            } else if score > 0.1 {
                ACCENT
            } else {
                TEXT_DIM
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
                    Style::default().fg(ACCENT_DIM),
                ),
                Span::styled(format!("{:.3}  ", score), Style::default().fg(color)),
                Span::styled(display_name, Style::default().fg(color)),
            ]));
        } else {
            lines.push(Line::from(vec![Span::styled(
                format!("{prefix}{connector}{}", node.name),
                Style::default().fg(ACCENT_DIM),
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
    let active_total = app.active_ctx_in + app.active_ctx_out;
    let active_pct = if app.context_max_tokens > 0 {
        (active_total as f64 / app.context_max_tokens as f64) * 100.0
    } else {
        0.0
    };

    let lines = vec![
        Line::from(vec![Span::styled(" ▌ Session", accent_bold())]),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!(" Model: {}", app.model),
            Style::default().fg(TEXT),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!(" Tokens in:  {}", app.tokens_in),
            Style::default().fg(TEXT_DIM),
        )]),
        Line::from(vec![Span::styled(
            format!(" Tokens out: {}", app.tokens_out),
            Style::default().fg(TEXT_DIM),
        )]),
        Line::from(vec![Span::styled(
            format!(" Total:      {}", total),
            Style::default().fg(TEXT),
        )]),
        Line::from(vec![Span::styled(
            format!(" Active Ctx: {} ({:.0}%)", active_total, active_pct),
            Style::default().fg(TEXT),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!(" Est. cost:  ${:.4}", app.cost_usd),
            Style::default().fg(WARN),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!(" Msgs: {}", app.messages.len()),
            Style::default().fg(TEXT_DIM),
        )]),
    ];

    frame.render_widget(Paragraph::new(lines).block(panel_block()), area);
}

fn draw_mcp_tab(frame: &mut Frame, area: Rect, app: &App) {
    let mut lines = vec![
        Line::from(vec![Span::styled(" ▌ MCP Servers", accent_bold())]),
        Line::from(""),
    ];

    if app.mcp_servers.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            " No MCP servers configured",
            Style::default().fg(TEXT_DIM),
        )]));
    } else {
        for (name, connected) in &app.mcp_servers {
            let (icon, color) = if *connected {
                (" ●", SUCCESS)
            } else {
                (" ○", ERROR)
            };
            lines.push(Line::from(vec![
                Span::styled(icon, Style::default().fg(color)),
                Span::styled(format!(" {name}"), Style::default().fg(TEXT)),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        " Configure in config.toml",
        Style::default().fg(TEXT_DIM),
    )]));

    frame.render_widget(Paragraph::new(lines).block(panel_block()), area);
}

fn draw_todos_tab(frame: &mut Frame, area: Rect, app: &App) {
    let done_count = app.todos.iter().filter(|t| t.done).count();
    let total = app.todos.len();
    let pct = (done_count * 100).checked_div(total).unwrap_or(0);

    let mut lines = vec![
        Line::from(vec![Span::styled(
            format!(" ▌ Progress  {}/{}  ({}%)", done_count, total, pct),
            accent_bold(),
        )]),
        Line::from(""),
    ];

    if app.todos.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            " No todos found in conversation.",
            Style::default().fg(TEXT_DIM),
        )]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            " Use checkboxes in prompts:",
            Style::default().fg(TEXT_DIM),
        )]));
        lines.push(Line::from(vec![Span::styled(
            " - [ ] Task name",
            Style::default().fg(ACCENT_DIM),
        )]));
    } else {
        for item in &app.todos {
            let (icon, color) = if item.done {
                (" ✔ ", SUCCESS)
            } else {
                (" ○ ", TEXT_DIM)
            };
            lines.push(Line::from(vec![
                Span::styled(icon, Style::default().fg(color)),
                Span::styled(&item.text, Style::default().fg(color)),
            ]));
        }
    }

    frame.render_widget(Paragraph::new(lines).block(panel_block()), area);
}

fn draw_splash(frame: &mut Frame, area: Rect) {
    let art = ascii_art::splash_display(area.width, area.height);
    let total = art.len() as u16;
    let top_pad = area.height.saturating_sub(total) / 2;
    let mut content: Vec<Line> = Vec::with_capacity(art.len() + top_pad as usize);
    for _ in 0..top_pad {
        content.push(Line::from(""));
    }
    content.extend(art);
    frame.render_widget(Paragraph::new(content).alignment(Alignment::Left), area);
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
            .style(Style::default().fg(ACCENT_DIM)),
        scrollbar_area,
        &mut s,
    );
}

fn build_message_lines(messages: &VecDeque<Message>, width: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let content_width = width.saturating_sub(2);

    for msg in messages {
        let (glyph, glyph_color) = match msg.role {
            MessageRole::User => ("▌ ", ACCENT_LIGHT),
            MessageRole::Assistant => ("▎ ", ACCENT_DIM),
            MessageRole::System => ("▏ ", TEXT_DIM),
        };

        let mut body: Vec<Line<'static>> = Vec::new();

        match msg.role {
            MessageRole::User => {
                for w in wrap_text(&msg.text, content_width) {
                    body.push(Line::from(Span::styled(
                        w,
                        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
                    )));
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
                                    body.push(hl);
                                }
                            }
                            code_buffer.clear();
                            in_code_block = false;
                            body.push(Line::from(Span::styled(
                                text_line.to_string(),
                                Style::default().fg(ACCENT_DIM),
                            )));
                        } else {
                            in_code_block = true;
                            code_lang =
                                trimmed.strip_prefix("```").unwrap_or("").trim().to_string();
                            body.push(Line::from(Span::styled(
                                text_line.to_string(),
                                Style::default().fg(ACCENT_DIM),
                            )));
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
                        body.push(Line::from(render_markdown_line(text_line)));
                    } else {
                        for w in wrap_text(text_line, content_width) {
                            if w.is_empty() {
                                body.push(Line::from(""));
                            } else {
                                body.push(Line::from(render_inline_markdown(&w)));
                            }
                        }
                    }
                }
            }
            MessageRole::System => {
                let is_error = msg.text.contains("Error") || msg.text.contains("error");
                let color = if is_error { ERROR } else { TEXT_DIM };
                for w in wrap_text(&msg.text, content_width) {
                    body.push(Line::from(Span::styled(w, Style::default().fg(color))));
                }
            }
        }

        for mut line in body {
            let mut spans = vec![Span::styled(glyph, Style::default().fg(glyph_color))];
            spans.append(&mut line.spans);
            lines.push(Line::from(spans));
        }
        lines.push(Line::from(""));
    }
    lines
}

fn render_markdown_line(line: &str) -> Vec<Span<'static>> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return vec![Span::raw("")];
    }
    if let Some(rest) = trimmed.strip_prefix("### ") {
        return vec![Span::styled(
            rest.to_string(),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )];
    }
    if let Some(rest) = trimmed.strip_prefix("## ") {
        return vec![Span::styled(
            rest.to_string(),
            Style::default()
                .fg(ACCENT_LIGHT)
                .add_modifier(Modifier::BOLD),
        )];
    }
    if let Some(rest) = trimmed.strip_prefix("# ") {
        return vec![Span::styled(
            rest.to_string(),
            Style::default()
                .fg(ACCENT_LIGHT)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )];
    }
    if let Some(content) = trimmed.strip_prefix("> ") {
        return vec![Span::styled(
            format!("│ {content}"),
            Style::default()
                .fg(ACCENT_DIM)
                .add_modifier(Modifier::ITALIC),
        )];
    }
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        let content = trimmed[2..].to_string();
        return vec![
            Span::styled("  • ", Style::default().fg(ACCENT)),
            Span::styled(content, Style::default().fg(TEXT)),
        ];
    }
    if let Some(cap) = numbered_prefix(trimmed) {
        return vec![Span::styled(cap, Style::default().fg(TEXT))];
    }
    if trimmed == "---" || trimmed == "***" || trimmed == "___" {
        let bar = "─".repeat(40);
        return vec![Span::styled(bar, Style::default().fg(ACCENT_DIM))];
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
    let base = Style::default().fg(TEXT);

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
                    Style::default().fg(ACCENT_LIGHT).bg(SURFACE_LIGHT),
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
    let (text_color, border_color) = if app.streaming {
        (ACCENT_DIM, ACCENT_DIM)
    } else {
        (TEXT, ACCENT)
    };
    let mut input = app.input.clone();
    input.set_style(Style::default().fg(text_color));
    input.set_block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(border_color))
            .title(Line::from(" ◆ message ").style(accent_bold())),
    );
    frame.render_widget(&input, area);

    if !app.streaming {
        frame.set_cursor_position((area.x + app.input.cursor().1 as u16, area.y + 1));
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
        "opencode" => (0.0, 0.0),
        "ollama" => (0.0, 0.0),
        _ => (1.0, 5.0),
    }
}

fn model_context_limit(model: &str) -> u64 {
    let model_lower = model.to_lowercase();
    if model_lower.contains("gemini") {
        1_048_576
    } else if model_lower.contains("gpt-4.1") {
        1_047_576
    } else if model_lower.contains("deepseek") || model_lower.contains("v4") {
        1_048_576
    } else if model_lower.contains("claude")
        || model_lower.contains("sonnet")
        || model_lower.contains("opus")
        || model_lower.contains("haiku")
    {
        200_000
    } else if model_lower.contains("ollama") {
        8_192
    } else {
        128_000
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
            Err(_) => spans.push(Span::styled(line.clone(), Style::default().fg(TEXT_DIM))),
        }
        result.push(Line::from(spans));
    }
    result
}
