use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};
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
}

pub enum ChatLine {
    User(String),
    Assistant(String),
    ToolProgress(String),
    ToolResult(String),
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
        }
    }

    pub fn add_message(&mut self, line: ChatLine) {
        self.messages.push_back(line);
        if self.messages.len() > 1000 {
            self.messages.pop_front();
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                if !self.input.is_empty() {
                    self.add_message(ChatLine::User(self.input.clone()));
                    self.input.clear();
                    self.cursor = 0;
                }
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.running = false;
            }
            KeyCode::Char(c) => {
                self.input.insert(self.cursor, c);
                self.cursor += 1;
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    self.input.remove(self.cursor - 1);
                    self.cursor -= 1;
                }
            }
            KeyCode::Delete => {
                if self.cursor < self.input.len() {
                    self.input.remove(self.cursor);
                }
            }
            KeyCode::Left => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
            }
            KeyCode::Right => {
                if self.cursor < self.input.len() {
                    self.cursor += 1;
                }
            }
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.input.len(),
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_add(5);
            }
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_sub(5);
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

    draw_header(frame, chunks[0], app);
    draw_messages(frame, chunks[1], app);
    draw_input(frame, chunks[2], app);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let text = Span::styled(
        format!(
            " zuc1fer | {} | in:{} out:{} | {}",
            app.model, app.tokens_in, app.tokens_out, app.status
        ),
        Style::default()
            .fg(Color::Black)
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(Paragraph::new(text), area);
}

fn draw_messages(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .messages
        .iter()
        .rev()
        .skip(app.scroll)
        .take(area.height as usize)
        .rev()
        .map(|line| match line {
            ChatLine::User(text) => {
                let content = vec![Line::from(vec![
                    Span::styled("> ", Style::default().fg(Color::Cyan)),
                    Span::styled(text, Style::default().fg(Color::White)),
                ])];
                ListItem::new(content)
            }
            ChatLine::Assistant(text) => {
                let lines: Vec<Line> = text
                    .lines()
                    .map(|l| Line::from(vec![Span::styled(l, Style::default().fg(Color::Green))]))
                    .collect();
                ListItem::new(lines)
            }
            ChatLine::ToolProgress(text) => {
                let content = vec![Line::from(vec![Span::styled(
                    format!("  ⚙ {text}"),
                    Style::default().fg(Color::Yellow),
                )])];
                ListItem::new(content)
            }
            ChatLine::ToolResult(text) => {
                let lines: Vec<Line> = text
                    .lines()
                    .take(4)
                    .map(|l| {
                        Line::from(vec![Span::styled(
                            format!("    {l}"),
                            Style::default().fg(Color::DarkGray),
                        )])
                    })
                    .collect();
                ListItem::new(lines)
            }
            ChatLine::Status(text) => {
                let content = vec![Line::from(vec![Span::styled(
                    format!("  — {text}"),
                    Style::default().fg(Color::DarkGray),
                )])];
                ListItem::new(content)
            }
            ChatLine::Error(text) => {
                let content = vec![Line::from(vec![Span::styled(
                    format!("  ✗ {text}"),
                    Style::default().fg(Color::Red),
                )])];
                ListItem::new(content)
            }
        })
        .collect();

    let messages = List::new(items).block(Block::default().borders(Borders::NONE));
    frame.render_widget(messages, area);
}

fn draw_input(frame: &mut Frame, area: Rect, app: &App) {
    let prompt = format!("> {}", app.input);
    let cursor_pos = app.cursor + 2;

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray));

    let paragraph = Paragraph::new(prompt)
        .block(block)
        .style(Style::default().fg(Color::White));

    frame.render_widget(paragraph, area);

    if app.cursor < app.input.len() || app.input.is_empty() {
        frame.set_cursor_position((area.x + cursor_pos as u16, area.y + 1));
    }
}
