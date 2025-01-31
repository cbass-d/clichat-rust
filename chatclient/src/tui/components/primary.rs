use super::component::{Component, ComponentRender, RenderProps};
use crate::state_handler::{Action, ClientState};

use super::TextType;
use crossterm::event::KeyEvent;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListDirection},
    Frame,
};
use tokio::sync::mpsc::UnboundedSender;

pub struct Primary {
    print_buffer: Vec<TextType>,
    title: String,
}

impl Component for Primary {
    fn new(state: &ClientState, _action_tx: UnboundedSender<Action>) -> Self
    where
        Self: Sized,
    {
        Self {
            print_buffer: state.notifications.clone(),
            title: state.current_server.clone(),
        }
    }

    fn update(self, state: &ClientState) -> Self
    where
        Self: Sized,
    {
        Self {
            print_buffer: state.notifications.clone(),
            title: state.current_server.clone(),
        }
    }

    fn handle_key_event(&mut self, _key: KeyEvent) {}
}

impl ComponentRender<RenderProps> for Primary {
    fn render(&self, frame: &mut Frame, props: RenderProps) {
        let title = match self.title.is_empty() {
            true => format!("CLI CHAT RUST"),
            false => self.title.clone(),
        };

        let mut rev_buffer = self.print_buffer.clone();
        rev_buffer.reverse();
        let text = List::new(
            rev_buffer
                .into_iter()
                .map(|line| match line {
                    TextType::Notification { text } => {
                        let style = Style::new().fg(Color::Blue).add_modifier(Modifier::BOLD);

                        Line::from(text).style(style)
                    }
                    TextType::Error { text } => {
                        let style = Style::new()
                            .fg(Color::LightRed)
                            .add_modifier(Modifier::BOLD);

                        Line::from(text).style(style)
                    }
                    TextType::Listing { text } => {
                        let style = Style::new()
                            .fg(Color::White)
                            .add_modifier(Modifier::UNDERLINED);

                        Line::from(text).style(style)
                    }
                    TextType::PrivateMessage { text } => {
                        let style = Style::new().fg(Color::White).add_modifier(Modifier::BOLD);

                        Line::from(text).style(style)
                    }
                    TextType::RoomMessage { text } => {
                        let style = Style::new().fg(Color::White).add_modifier(Modifier::BOLD);

                        Line::from(text).style(style)
                    }
                })
                .collect::<Vec<_>>(),
        )
        .direction(ListDirection::BottomToTop)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .fg(props.border_color),
        );

        frame.render_widget(text, props.area);
    }
}
