use crate::state_handler::{Action, ClientState};
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, style::Color, Frame};
use tokio::sync::mpsc::UnboundedSender;

pub struct RenderProps {
    pub area: Rect,
    pub border_color: Color,
}

pub trait Component {
    fn new(state: &ClientState, action_tx: UnboundedSender<Action>) -> Self
    where
        Self: Sized;

    fn update(self, state: &ClientState) -> Self
    where
        Self: Sized;

    fn handle_key_event(&mut self, key: KeyEvent);
}

pub trait ComponentRender<Props> {
    fn render(&self, frame: &mut Frame, props: Props);
}
