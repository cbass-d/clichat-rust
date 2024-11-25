use super::component::{Component, ComponentRender, RenderProps};
use crate::state_handler::{action::Action, state::State};

use crossterm::event::KeyEvent;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListDirection},
    Frame,
};
use tokio::sync::mpsc::UnboundedSender;

pub struct Primary {
    print_buffer: Vec<String>,
}

impl Primary {}

impl Component for Primary {
    fn new(state: &State, _action_tx: UnboundedSender<Action>) -> Self
    where
        Self: Sized,
    {
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

    fn handle_key_event(&mut self, _key: KeyEvent) {}
}

impl ComponentRender<RenderProps> for Primary {
    fn render(&self, frame: &mut Frame, props: RenderProps) {
        let mut rev_buffer = self.print_buffer.clone();
        rev_buffer.reverse();
        let text = List::new(
            rev_buffer
                .into_iter()
                .map(|line| Text::from(line))
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
