pub mod app_router;
pub mod components;

use crate::state_handler::Action;

use color_eyre::eyre::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::{FutureExt, StreamExt};
use ratatui::prelude::*;
use std::io::{self, Stdout};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

// Enum used to determine how to stylize the text
// printed out
#[derive(Clone)]
pub enum TextType {
    Notification { text: String },
    Error { text: String },
    RoomMessage { text: String },
    PrivateMessage { text: String },
    Listing { text: String },
}

pub struct Tui {
    pub action_tx: UnboundedSender<Action>,
}

#[derive(Clone, Copy, Debug)]
pub enum Event {
    Error,
    Tick,
    Key(KeyEvent),
}

pub struct EventHandler {
    _tx: UnboundedSender<Event>,
    rx: UnboundedReceiver<Event>,
}

impl EventHandler {
    pub fn new() -> Self {
        let tick_rate = std::time::Duration::from_millis(200);

        let (tx, rx) = mpsc::unbounded_channel();
        let _tx = tx.clone();

        let _task = tokio::spawn(async move {
            let mut reader = crossterm::event::EventStream::new();
            let mut interval = tokio::time::interval(tick_rate);
            loop {
                let delay = interval.tick();
                let crossterm_event = reader.next().fuse();
                tokio::select! {
                    maybe_event = crossterm_event => {
                        match maybe_event {
                            Some(Ok(event)) => {
                                if let crossterm::event::Event::Key(key) = event {
                                    if key.kind == crossterm::event::KeyEventKind::Press {
                                        tx.send(Event::Key(key)).unwrap();
                                    }
                                }
                            }
                            Some(Err(_)) => {
                                tx.send(Event::Error).unwrap();
                            }
                            None => {},
                        }
                    },
                    _ = delay => {
                        tx.send(Event::Tick).unwrap();
                    }
                }
            }
        });

        Self { _tx, rx }
    }

    pub async fn next(&mut self) -> Result<Event> {
        self.rx
            .recv()
            .await
            .ok_or(color_eyre::eyre::eyre!("Unable to get event"))
    }
}

impl Tui {
    pub fn new() -> (Self, UnboundedReceiver<Action>, EventHandler) {
        let (action_tx, action_rx) = mpsc::unbounded_channel::<Action>();
        let event_handler = EventHandler::new();

        (Self { action_tx }, action_rx, event_handler)
    }

    pub fn setup_terminal() -> Terminal<CrosstermBackend<Stdout>> {
        let mut stdout = io::stdout();

        enable_raw_mode().unwrap();

        execute!(stdout, EnterAlternateScreen, EnableMouseCapture).unwrap();

        Terminal::new(CrosstermBackend::new(stdout)).unwrap()
    }

    pub fn teardown_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) {
        disable_raw_mode().unwrap();

        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )
        .unwrap();

        terminal.show_cursor().unwrap()
    }
}
