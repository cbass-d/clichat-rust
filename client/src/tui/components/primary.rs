use super::component::{Component, ComponentRender, RenderProps};
use crate::state_handler::{
    action::Action,
    state::{ConnectionStatus, State},
};

use crossterm::event::KeyEvent;
use ratatui::{
    prelude::*,
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListDirection, ListItem, Paragraph},
    Frame,
};
use tokio::sync::mpsc::UnboundedSender;

pub struct Primary {
    print_buffer: Vec<String>,
}

impl Primary {
    pub fn add_to_buffer(&mut self, to_add: String) {
        self.print_buffer.push(to_add);
        self.print_buffer.reverse();
    }
}

impl Component for Primary {
    fn new(state: &State, action_tx: UnboundedSender<Action>) -> Self
    where
        Self: Sized,
    {
        let print_buffer: Vec<String> = vec![];
        Self {
            print_buffer: state.notifications.clone(),
        }
    }

    fn update(self, state: &State) -> Self
    where
        Self: Sized,
    {
        Self {
            print_buffer: state.notifications.clone(),
        }
    }

    fn handle_key_event(&mut self, key: KeyEvent) {}
}

impl ComponentRender<RenderProps> for Primary {
    fn render(&self, frame: &mut Frame, props: RenderProps) {
        let height = frame.area().height;
        let mut rev_buffer = self.print_buffer.clone();
        rev_buffer.reverse();
        let text = List::new(
            rev_buffer
                .clone()
                .into_iter()
                .map(|line| Line::from(Span::raw(line.clone())))
                .collect::<Vec<_>>(),
        )
        .style(Style::default().fg(Color::Green))
        .direction(ListDirection::BottomToTop)
        .block(
            Block::default()
                .title("CLI CHAT RUST")
                .borders(Borders::ALL)
                .fg(props.border_color),
        );

        frame.render_widget(text, props.area);
    }
}
