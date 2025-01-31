use anyhow::{anyhow, Result};

use super::TextType;
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
    pub notifications: Vec<TextType>,
    pub exit: bool,
}

impl Default for ClientState {
    fn default() -> ClientState {
        let mut startup_notifications = vec![];
        startup_notifications.push(TextType::Notification {
            text: String::from("---- To quit use /quit ----"),
        });

        startup_notifications.push(TextType::Notification {
            text: String::from("[*] No nickname set. To set one use the \"/name\" command"),
        });

        startup_notifications.push(TextType::Notification {
            text: String::from("[*] To see list of available commands use /help"),
        });

        ClientState {
            connection_status: ConnectionStatus::Unitiliazed,
            current_server: String::new(),
            username: String::new(),
            session_id: std::u64::MAX,
            notifications: startup_notifications,
            exit: false,
        }
    }
}

impl ClientState {
    pub fn push_notification(&mut self, notification: TextType) {
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

                self.push_notification(TextType::Error {
                    text: format!("[-] {failed_cmd} failed: {error}"),
                });

                match failed_cmd.as_ref() {
                    "register" => {
                        self.terminate_connection();

                        self.push_notification(TextType::Error {
                            text: String::from("[-] Connection to server closed"),
                        });

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

                self.push_notification(TextType::Notification {
                    text: format!("[+] Registered as {username}"),
                });
            }
            MessageType::ChangedName => {
                let new_username = body.arg.unwrap();
                let old_username = body.content.unwrap();

                self.username = new_username.clone();

                self.push_notification(TextType::Notification {
                    text: format!(
                        "[+] Name changed from \"{old_username}\" to \"{new_username}\"",
                    ),
                });
            }
            MessageType::UserRooms => {
                let content = body.content.unwrap();
                let mut rooms = Vec::new();
                if content.contains(",") {
                    rooms = content.split(",").map(|s| s.to_string()).collect();
                } else {
                    rooms.push(content);
                }

                self.push_notification(TextType::Notification {
                    text: String::from("[+] List of joined rooms"),
                });

                for room in rooms {
                    self.push_notification(TextType::Listing {
                        text: format!("[{room}]"),
                    });
                }

                self.push_notification(TextType::Notification {
                    text: String::from("[-] End of list"),
                });
            }
            MessageType::AllRooms => {
                let content = body.content.unwrap();
                let mut rooms = Vec::new();
                if content.contains(",") {
                    rooms = content.split(",").map(|s| s.to_string()).collect();
                } else {
                    rooms.push(content);
                }

                self.push_notification(TextType::Notification {
                    text: String::from("[+] List of all rooms"),
                });

                for room in rooms {
                    self.push_notification(TextType::Listing {
                        text: format!("[{room}]"),
                    });
                }

                self.push_notification(TextType::Notification {
                    text: String::from("[-] End of list"),
                });
            }
            MessageType::Users => {
                let content = body.content.unwrap();
                let mut users = Vec::new();
                if content.contains(",") {
                    users = content.split(",").map(|s| s.to_string()).collect();
                } else {
                    users.push(content);
                }

                self.push_notification(TextType::Notification {
                    text: String::from("[+] List users"),
                });

                for user in users {
                    self.push_notification(TextType::Listing {
                        text: format!("[{user}]"),
                    });
                }

                self.push_notification(TextType::Notification {
                    text: String::from("[-] End of list"),
                });
            }
            MessageType::Joined => {
                let room = body.arg.unwrap();
                self.push_notification(TextType::Notification {
                    text: format!("[+] Joined [{room}] room"),
                });
            }
            MessageType::LeftRoom => {
                let room = body.arg.unwrap();
                self.push_notification(TextType::Notification {
                    text: format!("[+] Left [{room}] room"),
                });
            }
            MessageType::CreatedRoom => {
                let room = body.arg.unwrap();
                self.push_notification(TextType::Notification {
                    text: format!("[+] Created [{room}] room"),
                });
            }
            MessageType::RoomMessage => {
                let room = body.arg.unwrap();
                let content = body.content.unwrap();
                self.push_notification(TextType::RoomMessage {
                    text: format!("[{room}] {content}"),
                });
            }
            MessageType::IncomingMsg => {
                let content = body.content.unwrap();
                self.push_notification(TextType::PrivateMessage {
                    text: format!("{content}"),
                });
            }
            MessageType::OutgoingMsg => {
                let receiver = body.arg.unwrap();
                let content = body.content.unwrap();
                self.push_notification(TextType::PrivateMessage {
                    text: format!("to {receiver}: {content}"),
                });
            }
            _ => {}
        }

        Ok(())
    }
}
