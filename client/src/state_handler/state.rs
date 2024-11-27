#[derive(Clone, PartialEq)]
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
    name: String,
    pub notifications: Vec<String>,
    pub exit: bool,
}

impl Default for State {
    fn default() -> Self {
        let mut startup_notices = vec![String::from("---To quit use \"/quit\"---")];

        startup_notices
            .push("[*] No nickname set. To set one use the \"/name\" command".to_string());
        startup_notices.push("\tExample: /name jon".to_string());

        startup_notices
            .push("[*] Not connected to server. To connect use \"/connect\" command".to_string());
        startup_notices.push("\tExample: /connect 127.0.0.1:6667".to_string());
        startup_notices.push("To send message to server: /sendto [arg] [msg]".to_string());

        State {
            connection_status: ConnectionStatus::Unitiliazed,
            current_server: String::new(),
            name: String::from("anon"), // Default name
            notifications: startup_notices,
            exit: false,
        }
    }
}

impl State {
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    pub fn get_name(&self) -> String {
        self.name.clone()
    }

    pub fn set_server(&mut self, server: String) {
        self.current_server = server;
    }

    pub fn get_server(&self) -> String {
        self.current_server.clone()
    }

    pub fn push_notification(&mut self, notification: String) {
        self.notifications.push(notification);
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
