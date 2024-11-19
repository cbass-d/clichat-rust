use crossterm::event::{KeyCode, KeyCode::Char, KeyEvent, KeyEventKind};
use std::net::TcpStream;

#[derive(Clone)]
pub enum ConnectionStatus {
    Established,
    Connecting,
    Bricked,
    Unitiliazed,
}

#[derive(Clone)]
pub struct State {
    connection_status: ConnectionStatus,
    current_server: String,
    pub notifications: Vec<String>,
    pub exit: bool,
}

impl Default for State {
    fn default() -> Self {
        State {
            connection_status: ConnectionStatus::Unitiliazed,
            current_server: String::new(),
            notifications: vec![
                String::from("[-] Not connected to a server"),
                String::from("[*] Enter server address (ex. 127.0.0.1:6667)"),
            ],
            exit: false,
        }
    }
}

impl State {
    pub fn set_server(&mut self, server: String) {
        self.current_server = server;
    }

    pub fn push_notification(&mut self, notification: String) {
        self.notifications.push(notification.clone());
    }

    pub fn set_connection_status(&mut self, status: ConnectionStatus) {
        self.connection_status = status;
    }

    pub fn get_connection_status(&self) -> ConnectionStatus {
        self.connection_status.clone()
    }

    pub fn exit(&mut self) {
        self.exit = true;
    }
}
