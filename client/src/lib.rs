pub mod state_handler;

use anyhow::{anyhow, Result};
use tokio::net::{
    tcp::{OwnedReadHalf, OwnedWriteHalf},
    TcpStream,
};

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

    pub fn handle_failure(
        &mut self,
        failed_cmd: String,
        connection_handle: Option<(OwnedReadHalf, OwnedWriteHalf)>,
    ) -> Option<(OwnedReadHalf, OwnedWriteHalf)> {
        match failed_cmd.as_str() {
            "register" => {
                self.terminate_connection();
                self.push_notification("[-] Connection to server closed".to_string());

                return None;
            }
            _ => {}
        }

        connection_handle
    }

    pub fn handle_message(&mut self, message: String) -> Result<()> {
        if let Some(message) = common::unpack_message(&message) {
            match message.cmd.as_str() {
                "registered" => {
                    let registered_name = message.arg.unwrap();
                    let id = message.content.unwrap();
                    let id = id.parse::<u64>().unwrap();
                    self.username = registered_name.clone();
                    self.session_id = id;

                    let notification = format!("[+] Registered as [{0}] #{1}", registered_name, id);
                    self.push_notification(notification);

                    Ok(())
                }
                "joined" => {
                    let room = message.arg.unwrap();

                    let notification = format!("[+] Joined [{0}] room", room);
                    self.push_notification(notification);

                    Ok(())
                }
                "roommessage" => {
                    let room = message.arg.unwrap();
                    let sender = message.sender;
                    let sender_id = message.id;
                    let content = message.content.unwrap();

                    let notification =
                        format!("[{0}] {1} #{2}: {3}", room, sender, sender_id, content);
                    self.push_notification(notification);

                    Ok(())
                }
                "rooms" => {
                    if let Some(content) = message.content {
                        let mut rooms: Vec<String> = Vec::new();
                        if content.contains(',') {
                            rooms = content.split(",").map(|s| s.to_string()).collect();
                        } else {
                            rooms.push(content);
                        }

                        self.push_notification("[+] List of joined rooms:".to_string());
                        for room in rooms {
                            let notification = format!("[{0}]", room);
                            self.push_notification(notification);
                        }
                        self.push_notification("[-] End of list".to_string());
                    } else {
                        self.push_notification("[-] No joined rooms".to_string());
                    }

                    Ok(())
                }
                "allrooms" => {
                    if let Some(content) = message.content {
                        let mut rooms: Vec<String> = Vec::new();
                        if content.contains(',') {
                            rooms = content.split(",").map(|s| s.to_string()).collect();
                        } else {
                            rooms.push(content);
                        }

                        self.push_notification("[+] List of all rooms:".to_string());
                        for room in rooms {
                            let notification = format!("[{0}]", room);
                            self.push_notification(notification);
                        }
                        self.push_notification("[-] End of list".to_string());
                    } else {
                        self.push_notification("[-] No rooms".to_string());
                    }

                    Ok(())
                }
                "users" => {
                    if let Some(content) = message.content {
                        let mut users: Vec<String> = Vec::new();
                        if content.contains(',') {
                            users = content.split(",").map(|s| s.to_string()).collect();
                        } else {
                            users.push(content);
                        }

                        self.push_notification("[+] List of server users:".to_string());
                        for user in users {
                            let notification = format!("[{0}]", user);
                            self.push_notification(notification);
                        }
                        self.push_notification("[-] End of list".to_string());
                    } else {
                        self.push_notification("[-] No users in sever".to_string());
                    }

                    Ok(())
                }
                "createdroom" => {
                    let new_room = message.arg.unwrap();

                    let notification = format!("[+] Created [{0}] room", new_room);
                    self.push_notification(notification);

                    Ok(())
                }
                "leftroom" => {
                    let room = message.arg.unwrap();

                    let notification = format!("[+] Left [{0}] room", room);
                    self.push_notification(notification);

                    Ok(())
                }
                "incomingmsg" => {
                    let sender = message.sender;
                    let content = message.content.unwrap();

                    let notification = format!("from [{0}]: {1}", sender, content);
                    self.push_notification(notification);

                    Ok(())
                }
                "outgoingmsg" => {
                    let receiver = message.arg.unwrap();
                    let content = message.content.unwrap();

                    let notification = format!("to [{0}]: {1}", receiver, content);
                    self.push_notification(notification);

                    Ok(())
                }
                "changedname" => {
                    let new_name = message.arg.unwrap();
                    let old_name = message.content.unwrap();
                    self.username = new_name.clone();

                    let notification =
                        format!("[+] Changed name from [{0}] to [{1}]", old_name, new_name);
                    self.push_notification(notification);

                    Ok(())
                }
                "failed" => {
                    let failed_cmd = message.arg.unwrap();
                    let error = message.content.unwrap();

                    let notification =
                        format!("[-] \"{0}\" command failed: {1}", failed_cmd, error);
                    self.push_notification(notification);

                    Err(anyhow!(ClientError::CommandFailed { failed_cmd }))
                }
                _ => Err(anyhow!(MessageError::InvalidCommand)),
            }
        } else {
            Err(anyhow!(MessageError::InvalidMessage))
        }
    }
}

#[derive(Debug)]
pub enum MessageError {
    InvalidCommand,
    InvalidMessage,
}

impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMessage => write!(f, "Invalid message received"),
            Self::InvalidCommand => write!(f, "Invalid response command found in message"),
        }
    }
}

impl std::error::Error for MessageError {}

#[derive(Debug)]
pub enum ClientError {
    CommandFailed { failed_cmd: String },
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CommandFailed { failed_cmd } => {
                write!(f, "Command failed: {}", failed_cmd)
            }
        }
    }
}

impl std::error::Error for ClientError {}
