use anyhow::{anyhow, Result};

use common::message::{Message, MessageType};

#[derive(Clone)]
pub enum ConnectionStatus {
    Unitiliazed,
    Established,
}

#[derive(Clone)]
pub struct ClientState {
    pub connection_status: ConnectionStatus,
    pub current_server: String,
    pub username: String,
    pub session_id: u64,
    pub notifications: Vec<String>,
    pub exit: bool,
}

impl Default for ClientState {
    fn default() -> ClientState {
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
        startup_notices.push("[*] To leave a joinded room: /leave room".to_string());
        startup_notices.push("[*] To list all rooms on server: /list allrooms".to_string());
        startup_notices.push("[*] To create a new room: /create ".to_string());
        startup_notices
            .push("[*] Private messages can be sent to users using \"/privmsg\"".to_string());
        startup_notices.push("    Example: /privmsg jon message".to_string());

        ClientState {
            connection_status: ConnectionStatus::Unitiliazed,
            current_server: String::new(),
            username: String::new(),
            session_id: std::u64::MAX,
            notifications: startup_notices,
            exit: false,
        }
    }
}

impl ClientState {
    pub fn push_notification(&mut self, notification: String) {
        self.notifications.push(notification);
    }

    pub fn exit(&mut self) {
        self.exit = true;
    }

    pub fn terminate_connection(&mut self) {
        self.connection_status = ConnectionStatus::Unitiliazed;
    }

    pub fn handle_message(&mut self, message: Message) -> Result<()> {
        let header = message.header;
        let message_type = header.message_type;
        let body = message.body;

        match message_type {
            MessageType::Failed => {
                let failed_cmd = body.arg.unwrap();
                let error = body.content.unwrap();
                self.push_notification(format!("[-] {failed_cmd} failed: {error}"));

                match failed_cmd.as_ref() {
                    "register" => {
                        self.terminate_connection();
                        self.push_notification(String::from("[-] Connection to server closed"));

                        return Err(anyhow!("Failed registration"));
                    }
                    _ => {}
                }
            }
            MessageType::Registered => {
                let given_id = body.arg.unwrap();
                let username = body.content.unwrap();

                self.session_id = given_id.parse::<u64>().unwrap();
                self.username = username.clone();

                self.push_notification(format!("[+] Registered as {username}"));
            }
            MessageType::ChangedName => {
                let new_username = body.arg.unwrap();
                let old_username = body.content.unwrap();

                self.username = new_username.clone();

                self.push_notification(format!(
                    "[+] Name changed from \"{old_username}\" to \"{new_username}\""
                ));
            }
            MessageType::UserRooms => {
                let content = body.content.unwrap();
                let mut rooms = Vec::new();
                if content.contains(",") {
                    rooms = content.split(",").map(|s| s.to_string()).collect();
                } else {
                    rooms.push(content);
                }

                self.push_notification(String::from("[+] List of joined rooms"));
                for room in rooms {
                    self.push_notification(format!("[{room}]"));
                }
                self.push_notification(String::from("[-] End of list"));
            }
            MessageType::AllRooms => {
                let content = body.content.unwrap();
                let mut rooms = Vec::new();
                if content.contains(",") {
                    rooms = content.split(",").map(|s| s.to_string()).collect();
                } else {
                    rooms.push(content);
                }

                self.push_notification(String::from("[+] List of all rooms"));
                for room in rooms {
                    self.push_notification(format!("[{room}]"));
                }
                self.push_notification(String::from("[-] End of list"));
            }
            MessageType::Users => {
                let content = body.content.unwrap();
                let mut users = Vec::new();
                if content.contains(",") {
                    users = content.split(",").map(|s| s.to_string()).collect();
                } else {
                    users.push(content);
                }

                self.push_notification(String::from("[+] List of users"));
                for user in users {
                    self.push_notification(format!("[{user}]"));
                }
                self.push_notification(String::from("[-] End of list"));
            }
            MessageType::Joined => {
                let room = body.arg.unwrap();
                self.push_notification(format!("[+] Joined [{room}] room"));
            }
            MessageType::LeftRoom => {
                let room = body.arg.unwrap();
                self.push_notification(format!("[+] Left [{room}] room"));
            }
            MessageType::CreatedRoom => {
                let room = body.arg.unwrap();
                self.push_notification(format!("[+] Created [{room}] room"));
            }
            MessageType::RoomMessage => {
                let room = body.arg.unwrap();
                let content = body.content.unwrap();
                self.push_notification(format!("[{room}] {content}"));
            }
            MessageType::IncomingMsg => {
                let content = body.content.unwrap();
                self.push_notification(content);
            }
            MessageType::OutgoingMsg => {
                let receiver = body.arg.unwrap();
                let content = body.content.unwrap();
                self.push_notification(format!("to {receiver}: {content}"));
            }
            _ => {}
        }

        Ok(())
    }
}
