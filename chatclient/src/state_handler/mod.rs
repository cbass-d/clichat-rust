mod action;
mod state;

pub use action::{parse_command, Action};
pub use state::{ClientState, ConnectionStatus};

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

pub struct StateHandler {
    pub state_tx: UnboundedSender<ClientState>,
}

impl StateHandler {
    pub fn new() -> (Self, UnboundedReceiver<ClientState>) {
        let (state_tx, state_rx) = mpsc::unbounded_channel::<ClientState>();

        (Self { state_tx }, state_rx)
    }
}
