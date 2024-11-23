use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    layout::Position,
    style::{Color, Style, Stylize},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use tokio::sync::mpsc::UnboundedSender;

use super::component::{Component, ComponentRender, RenderProps};
use crate::state_handler::{
    action::Action,
    state::{ConnectionStatus, State},
};

pub struct InputBox {
    char_index: usize,
    input: String,
    connection_status: ConnectionStatus,
    action_tx: UnboundedSender<Action>,
}

impl InputBox {
    pub fn cursor_left(&mut self) {
        let moved_left = self.char_index.saturating_sub(1);
        self.char_index = self.clamp_cursor(moved_left);
    }

    pub fn cursor_right(&mut self) {
        let moved_right = self.char_index.saturating_add(1);
        self.char_index = self.clamp_cursor(moved_right);
    }

    pub fn enter_char(&mut self, new_char: char) {
        let index = self.byte_index();
        self.input.insert(index, new_char);
        self.cursor_right();
    }

    fn byte_index(&self) -> usize {
        self.input
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.char_index)
            .unwrap_or(self.input.len())
    }

    pub fn delete_char(&mut self) {
        let not_leftmost = self.char_index != 0;
        if not_leftmost {
            let curr_index = self.char_index;
            let from_left = curr_index - 1;

            let before_del = self.input.chars().take(from_left);
            let after_del = self.input.chars().skip(curr_index);

            self.input = before_del.chain(after_del).collect();
            self.cursor_left();
        }
    }

    fn clamp_cursor(&self, new_pos: usize) -> usize {
        new_pos.clamp(0, self.input.chars().count())
    }

    fn reset_cursor(&mut self) {
        self.char_index = 0;
    }

    pub fn submit(&mut self) {
        if self.input == "q" || self.input == "quit" {
            self.action_tx.send(Action::Quit);
        } else if self.input == "disconnect" {
            self.action_tx.send(Action::Disconnect);
        } else {
            match self.connection_status {
                ConnectionStatus::Unitiliazed => {
                    let _ = self.action_tx.send(Action::Connect {
                        addr: self.input.trim().to_string(),
                    });
                }
                ConnectionStatus::Connecting => {}
                ConnectionStatus::Established => {
                    let _ = self.action_tx.send(Action::Send {
                        data: self.input.trim().to_string(),
                    });
                }
                ConnectionStatus::Bricked => {}
            }
        }

        self.input.clear();
        self.reset_cursor();
    }
}

impl Component for InputBox {
    fn new(state: &State, action_tx: UnboundedSender<Action>) -> Self {
        Self {
            char_index: 0,
            input: String::new(),
            connection_status: state.get_connection_status(),
            action_tx,
        }
    }

    fn update(self, state: &State) -> Self
    where
        Self: Sized,
    {
        Self {
            connection_status: state.get_connection_status(),
            ..self
        }
    }

    fn handle_key_event(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        match key.code {
            KeyCode::Char(to_insert) => {
                self.enter_char(to_insert);
            }
            KeyCode::Backspace => {
                self.delete_char();
            }
            KeyCode::Enter => {
                self.submit();
            }
            KeyCode::Left => {
                self.cursor_left();
            }
            KeyCode::Right => {
                self.cursor_right();
            }
            _ => {}
        }
    }
}

impl ComponentRender<RenderProps> for InputBox {
    fn render(&self, frame: &mut Frame, props: RenderProps) {
        let input = Paragraph::new(self.input.as_str())
            .style(Style::default().fg(Color::Green))
            .block(
                Block::default()
                    .title("User Input")
                    .borders(Borders::ALL)
                    .fg(props.border_color),
            );
        frame.render_widget(input, props.area);

        frame.set_cursor_position(Position::new(
            props.area.x + self.char_index as u16 + 1,
            props.area.y + 1,
        ));
    }
}
