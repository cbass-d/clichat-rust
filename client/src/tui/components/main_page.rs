use super::component::{Component, ComponentRender};
use super::input_box::{InputBox, RenderProps};
use super::primary::Primary;
use crate::state_handler::{action::Action, state::State};

use crossterm::event::KeyEvent;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    prelude::*,
    style::{Color, Style, Stylize},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};
use tokio::sync::mpsc::UnboundedSender;

pub struct MainPage {
    input_box: InputBox,
    primary: Primary,
}

impl MainPage {}

impl Component for MainPage {
    fn new(state: &State, action_tx: UnboundedSender<Action>) -> Self
    where
        Self: Sized,
    {
        Self {
            input_box: InputBox::new(state, action_tx.clone()),
            primary: Primary::new(state, action_tx),
        }
    }

    fn update(self, state: &State) -> Self
    where
        Self: Sized,
    {
        Self {
            input_box: self.input_box.update(state),
            primary: self.primary.update(state),
        }
    }

    fn handle_key_event(&mut self, key: KeyEvent) {
        self.input_box.handle_key_event(key);
    }
}

impl ComponentRender<()> for MainPage {
    fn render(&self, frame: &mut Frame, props: ()) {
        let constraints = Constraint::from_percentages([90, 10]);
        let layout = Layout::default()
            .constraints(constraints)
            .split(frame.area());
        self.input_box.render(
            frame,
            RenderProps {
                area: layout[1],
                border_color: Color::LightMagenta,
            },
        );

        self.primary.render(
            frame,
            RenderProps {
                area: layout[0],
                border_color: Color::LightMagenta,
            },
        );
    }
}
