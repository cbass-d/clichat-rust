#[allow(unused_imports)]
use ratatui::{
    backend::{Backend, CrosstermBackend},
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    Terminal,
};

use crate::ClientState;
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
