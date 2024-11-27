pub enum Action {
    Connect { addr: String },
    SetName { name: String },
    Disconnect,
    SendTo { arg: String, message: String },
    Join { room: String },
    List { opt: String },
    Quit,
    Invalid,
}

pub fn parse_command(string: String) -> Option<Action> {
    let mut tokens = string.split_whitespace();
    if let Some(cmd) = tokens.next() {
        if cmd.starts_with('/') {
            let cmd_name = &cmd[1..];

            match cmd_name {
                "name" => {
                    let name = match tokens.next() {
                        Some(name) => name.to_string(),
                        None => {
                            return None;
                        }
                    };
                    return Some(Action::SetName { name });
                }
                "connect" => {
                    let addr = match tokens.next() {
                        Some(addr) => addr.to_string(),
                        None => {
                            return None;
                        }
                    };

                    return Some(Action::Connect { addr });
                }
                "sendto" => {
                    let arg = match tokens.next() {
                        Some(arg) => arg.to_string(),
                        None => {
                            return None;
                        }
                    };
                    let mut message = String::new();
                    while let Some(part) = tokens.next() {
                        message += part;
                        message += " ";
                    }
                    message = message.trim().to_string();

                    if message == "" {
                        return None;
                    }
                    return Some(Action::SendTo { arg, message });
                }
                "list" => {
                    let opt = match tokens.next() {
                        Some(opt) => opt.to_string(),
                        None => {
                            return None;
                        }
                    };

                    return Some(Action::List { opt });
                }
                "join" => {
                    let room = match tokens.next() {
                        Some(room) => room.to_string(),
                        None => {
                            return None;
                        }
                    };

                    return Some(Action::Join { room });
                }
                "disconnect" => {
                    return Some(Action::Disconnect);
                }
                "quit" => {
                    return Some(Action::Quit);
                }
                _ => {}
            }
        }
    }

    return None;
}
