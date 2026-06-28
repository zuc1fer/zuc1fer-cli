use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::cell::Cell;
use std::collections::VecDeque;

pub struct App {
    pub messages: VecDeque<Message>,
    pub input: String,
    pub cursor: usize,
    pub status: String,
    pub model: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub running: bool,
    pub streaming: bool,
    pub repo_files: Vec<(String, f64)>,
    pub show_repo_panel: bool,
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

impl App {
    pub fn new(model: &str) -> Self {
        Self {
            messages: VecDeque::new(),
            input: String::new(),
            cursor: 0,
            status: String::from("Ready"),
            model: model.to_string(),
            tokens_in: 0,
            tokens_out: 0,
            running: true,
            streaming: false,
            repo_files: Vec::new(),
            show_repo_panel: false,
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
    }

    pub fn next_turn(&mut self) {
        let idx = self.messages.len();
        self.messages.push_back(Message {
            role: MessageRole::Assistant,
            text: String::new(),
        });
        self.last_assistant_idx = idx;
    }

    fn max_scroll(&self) -> usize {
        self.total_lines
            .get()
            .saturating_sub(self.view_height.get())
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.running = false;
            }
            KeyCode::Tab => {
                self.show_repo_panel = !self.show_repo_panel;
            }
            KeyCode::Char(c) if !self.streaming => {
                self.input.insert(self.cursor, c);
                self.cursor += 1;
            }
            KeyCode::Backspace if !self.streaming => {
                if self.cursor > 0 {
                    self.input.remove(self.cursor - 1);
                    self.cursor -= 1;
                }
            }
            KeyCode::Delete if !self.streaming => {
                if self.cursor < self.input.len() {
                    self.input.remove(self.cursor);
                }
            }
            KeyCode::Left if !self.streaming => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
            }
            KeyCode::Right if !self.streaming => {
                if self.cursor < self.input.len() {
                    self.cursor += 1;
                }
            }
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.input.len(),
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
            _ => {}
        }
    }
}

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(4),
            Constraint::Length(3),
        ])
        .split(area);

    let msg_area = chunks[1];

    if app.show_repo_panel && msg_area.width > 40 {
        let h_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(msg_area);

        app.view_height.set(h_chunks[0].height as usize);
        draw_messages(frame, h_chunks[0], app);
        draw_repo_panel(frame, h_chunks[1], app);
    } else {
        app.view_height.set(msg_area.height as usize);
        draw_messages(frame, msg_area, app);
    }

    draw_header(frame, chunks[0], app);
    draw_input(frame, chunks[2], app);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
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
    let panel_indicator = if app.show_repo_panel { " [Tab]" } else { "" };
    let text = format!(
        " zuc1fer | {} | {}in {}out | {}{}{}",
        app.model, app.tokens_in, app.tokens_out, app.status, scrolled, panel_indicator
    );
    frame.render_widget(
        Paragraph::new(text).style(Style::default().bg(Color::DarkGray).fg(Color::White)),
        area,
    );
}

fn draw_repo_panel(frame: &mut Frame, area: Rect, app: &App) {
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![Span::styled(
        " Repository Map ",
        Style::default().fg(Color::Cyan),
    )]));
    lines.push(Line::from(""));

    if app.repo_files.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            " (no data yet)",
            Style::default().fg(Color::DarkGray),
        )]));
    } else {
        for (path, score) in &app.repo_files {
            let line = if *score > 0.5 {
                Line::from(vec![Span::styled(
                    format!(" {:.3}  {}", score, path),
                    Style::default().fg(Color::White),
                )])
            } else if *score > 0.1 {
                Line::from(vec![Span::styled(
                    format!(" {:.3}  {}", score, path),
                    Style::default().fg(Color::Gray),
                )])
            } else {
                Line::from(vec![Span::styled(
                    format!(" {:.3}  {}", score, path),
                    Style::default().fg(Color::DarkGray),
                )])
            };
            lines.push(line);
        }
    }

    let panel = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(panel, area);
}

fn draw_messages(frame: &mut Frame, area: Rect, app: &App) {
    let width = area.width as usize;
    let max_lines = area.height as usize;

    let all_lines = build_message_lines(&app.messages, width);
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
    let indicator = if app.streaming { "~" } else { ">" };
    let text = format!("{} {}", indicator, app.input);

    frame.render_widget(
        Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .style(if app.streaming {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            }),
        area,
    );

    if !app.streaming {
        frame.set_cursor_position((area.x + app.cursor as u16 + 2, area.y + 1));
    }
}
