use crossterm::event::{KeyCode, KeyCode::Char, KeyEvent, KeyEventKind};

#[derive(Clone)]
enum ConnectionStatus {
    Established,
    Connecting,
    Bricked,
    Unitiliazed,
}

#[derive(Clone)]
pub struct State {
    connection_status: ConnectionStatus,
    current_server: String,
    pub exit: bool,
}

impl Default for State {
    fn default() -> Self {
        State {
            connection_status: ConnectionStatus::Unitiliazed,
            current_server: String::new(),
            exit: false,
        }
    }
}

impl State {
    pub fn set_server(&mut self, server: String) {
        self.current_server = server;
    }
    pub fn exit(&mut self) {
        self.exit = true;
    }
}
