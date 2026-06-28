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
    pub scroll: usize,
    pub running: bool,
    pub streaming: bool,
    pub view_height: Cell<usize>,
    last_assistant_idx: usize,
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
            scroll: 0,
            running: true,
            streaming: false,
            view_height: Cell::new(24),
            last_assistant_idx: 0,
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
        self.scroll = 0;
    }

    pub fn append_stream(&mut self, text: &str) {
        if let Some(msg) = self.messages.get_mut(self.last_assistant_idx) {
            msg.text.push_str(text);
        }
    }

    pub fn end_streaming(&mut self) {
        self.streaming = false;
        self.scroll = 0;
    }

    fn scroll_step(&self) -> usize {
        (self.view_height.get() / 2).max(1)
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.running = false;
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
            KeyCode::PageUp | KeyCode::Up => {
                self.scroll = self.scroll.saturating_add(self.scroll_step());
            }
            KeyCode::PageDown | KeyCode::Down => {
                self.scroll = self.scroll.saturating_sub(self.scroll_step());
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

    app.view_height.set(chunks[1].height as usize);

    draw_header(frame, chunks[0], app);
    draw_messages(frame, chunks[1], app);
    draw_input(frame, chunks[2], app);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let scrolled = if app.scroll > 0 {
        format!(" [^{}]", app.scroll)
    } else {
        String::new()
    };
    let text = format!(
        " zuc1fer | {} | {}in {}out | {}{}",
        app.model, app.tokens_in, app.tokens_out, app.status, scrolled
    );
    frame.render_widget(
        Paragraph::new(text).style(Style::default().bg(Color::DarkGray).fg(Color::White)),
        area,
    );
}

fn draw_messages(frame: &mut Frame, area: Rect, app: &App) {
    let mut lines: Vec<Line> = Vec::new();

    let max_lines = area.height.saturating_sub(2) as usize;

    for msg in app
        .messages
        .iter()
        .rev()
        .skip(app.scroll)
        .take(max_lines)
        .rev()
    {
        match msg.role {
            MessageRole::User => {
                lines.push(Line::from(vec![
                    Span::styled("> ", Style::default().fg(Color::Cyan)),
                    Span::styled(&msg.text, Style::default().fg(Color::White)),
                ]));
            }
            MessageRole::Assistant => {
                for text_line in msg.text.lines() {
                    lines.push(Line::from(vec![Span::styled(
                        text_line,
                        Style::default().fg(Color::Gray),
                    )]));
                }
            }
            MessageRole::System => {
                for text_line in msg.text.lines() {
                    lines.push(Line::from(vec![Span::styled(
                        format!("  {text_line}"),
                        Style::default().fg(Color::DarkGray),
                    )]));
                }
            }
        }
    }

    if lines.len() < max_lines {
        let blank = max_lines - lines.len();
        for _ in 0..blank {
            lines.push(Line::from(""));
        }
    }

    frame.render_widget(Paragraph::new(lines), area);
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
