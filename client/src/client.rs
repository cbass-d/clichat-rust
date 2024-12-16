use anyhow::{anyhow, Result};

use super::State;

#[derive(Debug)]
pub enum MessageError {
    InvalidCommand,
    InvalidMessage,
    CommandFailed { failed_cmd: String },
}

impl std::fmt::Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CommandFailed { failed_cmd } => {
                let literal = format!("{0} failed", failed_cmd);
                write!(f, "{}", literal)
            }
            Self::InvalidMessage => write!(f, "Invalid message received"),
            Self::InvalidCommand => write!(f, "Invalid response command found in message"),
        }
    }
}

impl std::error::Error for MessageError {}

pub fn handle_message(message: String, state: &mut State) -> Result<()> {
    if let Some(message) = common::unpack_message(&message) {
        match message.cmd.as_str() {
            "registered" => {
                let registered_name = message.arg.unwrap();
                let id = message.content.unwrap();
                let id = id.parse::<u64>().unwrap();
                state.set_name(registered_name.clone());
                state.set_session_id(id);

                let notification = format!("[+] Registered as [{0}] #{1}", registered_name, id);
                state.push_notification(notification);

                Ok(())
            }
            "joined" => {
                let room = message.arg.unwrap();

                let notification = format!("[+] Joined [{0}] room", room);
                state.push_notification(notification);

                Ok(())
            }
            "roommessage" => {
                let room = message.arg.unwrap();
                let sender = message.sender;
                let sender_id = message.id;
                let content = message.content.unwrap();

                let notification = format!("[{0}] {1} #{2}: {3}", room, sender, sender_id, content);
                state.push_notification(notification);

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

                    state.push_notification("[+] List of joined rooms:".to_string());
                    for room in rooms {
                        let notification = format!("[{0}]", room);
                        state.push_notification(notification);
                    }
                    state.push_notification("[-] End of list".to_string());
                } else {
                    state.push_notification("[-] No joined rooms".to_string());
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

                    state.push_notification("[+] List of all rooms:".to_string());
                    for room in rooms {
                        let notification = format!("[{0}]", room);
                        state.push_notification(notification);
                    }
                    state.push_notification("[-] End of list".to_string());
                } else {
                    state.push_notification("[-] No rooms".to_string());
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

                    state.push_notification("[+] List of server users:".to_string());
                    for user in users {
                        let notification = format!("[{0}]", user);
                        state.push_notification(notification);
                    }
                    state.push_notification("[-] End of list".to_string());
                } else {
                    state.push_notification("[-] No users in sever".to_string());
                }

                Ok(())
            }
            "createdroom" => {
                let new_room = message.arg.unwrap();

                let notification = format!("[+] Created [{0}] room", new_room);
                state.push_notification(notification);

                Ok(())
            }
            "leftroom" => {
                let room = message.arg.unwrap();

                let notification = format!("[+] Left [{0}] room", room);
                state.push_notification(notification);

                Ok(())
            }
            "incomingmsg" => {
                let sender = message.sender;
                let content = message.content.unwrap();

                let notification = format!("from [{0}]: {1}", sender, content);
                state.push_notification(notification);

                Ok(())
            }
            "outgoingmsg" => {
                let receiver = message.arg.unwrap();
                let content = message.content.unwrap();

                let notification = format!("to [{0}]: {1}", receiver, content);
                state.push_notification(notification);

                Ok(())
            }
            "changedname" => {
                let new_name = message.arg.unwrap();
                let old_name = message.content.unwrap();
                state.set_name(new_name.clone());

                let notification =
                    format!("[+] Changed name from [{0}] to [{1}]", old_name, new_name);
                state.push_notification(notification);

                Ok(())
            }
            "failed" => {
                let failed_cmd = message.arg.unwrap();
                let error = message.content.unwrap();

                let notification = format!("[-] \"{0}\" command failed: {1}", failed_cmd, error);
                state.push_notification(notification);

                Err(anyhow!(MessageError::CommandFailed { failed_cmd }))
            }
            _ => Err(anyhow!(MessageError::InvalidCommand)),
        }
    } else {
        Err(anyhow!(MessageError::InvalidMessage))
    }
}
