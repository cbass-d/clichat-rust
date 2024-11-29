#[derive(Clone, PartialEq)]
pub enum ConnectionStatus {
    Established,
    Connecting,
    Unitiliazed,
}

#[derive(Clone)]
pub struct State {
    connection_status: ConnectionStatus,
    registered: bool,
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
        startup_notices.push("    Example: /name jon".to_string());

        startup_notices
            .push("[*] Not connected to server. To connect use \"/connect\" command".to_string());
        startup_notices.push("    Example: /connect 127.0.0.1:6667".to_string());
        startup_notices.push("[*] Server has one default [#main] room".to_string());
        startup_notices.push("    To join use: /join main".to_string());
        startup_notices
            .push("    To send message to server room: /sendto {room} {msg}".to_string());
        startup_notices.push("    To change name on server use the \"/name\" command".to_string());
        startup_notices.push("    Username must be unique on server".to_string());
        startup_notices.push("    To list users: /list users".to_string());
        startup_notices.push("[*] To list joined rooms: /list rooms".to_string());

        State {
            connection_status: ConnectionStatus::Unitiliazed,
            registered: false,
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

    pub fn registered(&self) -> bool {
        self.registered
    }

    pub fn set_as_registered(&mut self) {
        self.registered = true;
    }

    pub fn exit(&mut self) {
        self.exit = true;
    }
}
