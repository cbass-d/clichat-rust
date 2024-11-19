use super::component::{Component, ComponentRender};
use super::input_box::RenderProps;
use crate::state_handler::{action::Action, state::State};

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
        Self {
            print_buffer: vec![String::from("Bleh"), String::from("glurp")],
        }
    }

    fn update(self, state: &State) -> Self
    where
        Self: Sized,
    {
        Self {
            print_buffer: self.print_buffer,
        }
    }

    fn handle_key_event(&mut self, key: KeyEvent) {}
}

impl ComponentRender<RenderProps> for Primary {
    fn render(&self, frame: &mut Frame, props: RenderProps) {
        let height = frame.area().height;
        let text = List::new(
            self.print_buffer
                .clone()
                .into_iter()
                .map(|line| Line::from(Span::raw(line.clone())))
                .collect::<Vec<_>>(),
        )
        .style(Style::default().fg(Color::Green))
        .direction(ListDirection::BottomToTop)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .fg(props.border_color),
        );

        frame.render_widget(text, props.area);
    }
}
