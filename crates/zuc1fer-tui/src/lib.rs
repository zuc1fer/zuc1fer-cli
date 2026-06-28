use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::cell::Cell;
use std::collections::VecDeque;

pub struct App {
    pub messages: VecDeque<ChatLine>,
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

pub enum ChatLine {
    User(String),
    Assistant(String),
    Status(String),
    Error(String),
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

    pub fn add_message(&mut self, line: ChatLine) {
        self.messages.push_back(line);
        if self.messages.len() > 500 {
            self.messages.pop_front();
        }
        if !self.streaming {
            self.scroll = 0;
        }
    }

    pub fn start_streaming(&mut self) {
        let idx = self.messages.len();
        self.messages.push_back(ChatLine::Assistant(String::new()));
        self.last_assistant_idx = idx;
        self.streaming = true;
        self.scroll = 0;
    }

    pub fn append_stream(&mut self, text: &str) {
        if let Some(ChatLine::Assistant(ref mut content)) = self.messages.get_mut(self.last_assistant_idx) {
            content.push_str(text);
        }
    }

    pub fn end_streaming(&mut self) {
        self.streaming = false;
        self.scroll = 0;
    }

    fn scroll_step(&self) -> usize {
        (self.view_height.get() / 2).max(1)
    }

    fn max_scroll(&self) -> usize {
        let total = self.messages.len();
        total.saturating_sub(self.view_height.get().saturating_sub(2))
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
                let step = self.scroll_step();
                self.scroll = (self.scroll + step).min(self.max_scroll());
            }
            KeyCode::PageDown | KeyCode::Down => {
                let step = self.scroll_step();
                self.scroll = self.scroll.saturating_sub(step);
            }
            _ => {}
        }
    }
}

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(frame.area());

    app.view_height.set(chunks[1].height as usize);

    draw_header(frame, chunks[0], app);
    draw_messages(frame, chunks[1], app);
    draw_input(frame, chunks[2], app);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let scrolled = if app.scroll > 0 {
        format!(" [scrolled {}]", app.scroll)
    } else {
        String::new()
    };
    let header = Span::styled(
        format!(
            " zuc1fer | {} | in:{} out:{} | {}{}",
            app.model, app.tokens_in, app.tokens_out, app.status, scrolled
        ),
        Style::default()
            .fg(Color::Black)
            .bg(Color::Gray)
            .add_modifier(Modifier::BOLD),
    );
    let p = Paragraph::new(header);
    frame.render_widget(p, area);
}

fn draw_messages(frame: &mut Frame, area: Rect, app: &App) {
    let mut lines: Vec<Line> = Vec::new();

    let visible = app
        .messages
        .iter()
        .rev()
        .skip(app.scroll)
        .take(area.height.saturating_sub(2) as usize);

    for msg in visible.rev() {
        match msg {
            ChatLine::User(text) => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "> ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(text, Style::default().fg(Color::White)),
                ]));
            }
            ChatLine::Assistant(text) => {
                for line in text.lines() {
                    lines.push(Line::from(vec![Span::styled(
                        line,
                        Style::default().fg(Color::Green),
                    )]));
                }
            }
            ChatLine::Status(text) => {
                lines.push(Line::from(vec![Span::styled(
                    format!("  - {text}"),
                    Style::default().fg(Color::DarkGray),
                )]));
            }
            ChatLine::Error(text) => {
                lines.push(Line::from(vec![Span::styled(
                    format!("  x {text}"),
                    Style::default().fg(Color::Red),
                )]));
            }
        }
        lines.push(Line::from(""));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

fn draw_input(frame: &mut Frame, area: Rect, app: &App) {
    let indicator = if app.streaming { "~" } else { ">" };
    let prompt = format!("{indicator} {}", app.input);

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray));

    let style = if app.streaming {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };

    let paragraph = Paragraph::new(prompt).block(block).style(style);

    frame.render_widget(paragraph, area);

    if !app.streaming {
        frame.set_cursor_position((area.x + app.cursor as u16 + 2, area.y + 1));
    }
}
