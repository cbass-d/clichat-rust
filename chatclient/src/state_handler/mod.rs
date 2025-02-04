mod action;
mod state;

pub use super::tui::TextType;
pub use action::{parse_command, Action};
pub use state::{ClientState, ConnectionStatus};

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

pub struct StateHandler {
    state_tx: UnboundedSender<()>,
}

impl StateHandler {
    pub fn new() -> (Self, UnboundedReceiver<()>) {
        let (state_tx, state_rx) = mpsc::unbounded_channel::<()>();

        (Self { state_tx }, state_rx)
    }

    pub fn updated(&self) {
        self.state_tx.send(()).unwrap();
    }
}
