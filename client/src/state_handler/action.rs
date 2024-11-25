pub enum Action {
    Connect { addr: String },
    SetName { name: String },
    Disconnect,
    Send { data: String },
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
                        None => return Some(Action::Invalid),
                    };
                    return Some(Action::SetName { name });
                }
                "connect" => {
                    let addr = match tokens.next() {
                        Some(addr) => addr.to_string(),
                        None => return Some(Action::Invalid),
                    };

                    return Some(Action::Connect { addr });
                }
                "send" => {
                    let mut msg = String::new();
                    while let Some(part) = tokens.next() {
                        msg += part;
                        msg += " ";
                    }
                    msg = msg.trim().to_string();

                    if msg == "" {
                        return Some(Action::Invalid);
                    }
                    return Some(Action::Send { data: msg });
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

    return Some(Action::Invalid);
}
