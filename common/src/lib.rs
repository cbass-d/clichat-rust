pub fn pack_message(cmd: &str, arg: Option<&str>, sender: &str, message: Option<&str>) -> String {
    let mut packed = String::from("!#");
    packed.push_str(cmd);

    if let Some(arg) = arg {
        packed.push('#');
        packed.push_str(arg);
    }

    packed.push('#');
    packed.push_str(sender);
    if let Some(message) = message {
        packed.push('#');
        packed.push_str(message);
    }
    packed.push_str("#!");

    packed
}

pub fn unpack_message(message: &str) -> Option<(&str, Option<&str>, &str, Option<&str>)> {
    if !message.starts_with("!") || !message.ends_with("!") {
        return None;
    }

    // Remove starting and ending !s
    let content = &message[1..message.len() - 1];
    let tokens: Vec<&str> = content.split('#').filter(|s| !s.is_empty()).collect();

    match tokens[0] {
        "register" => {
            let cmd = tokens[0];
            let arg = tokens[1];
            let sender = tokens[2];
            Some((cmd, Some(arg), sender, None))
        }
        "registered" => {
            let cmd = tokens[0];
            let arg = tokens[1];
            let sender = tokens[1];
            let message = tokens[3];

            Some((cmd, Some(arg), sender, Some(message)))
        }
        "join" => {
            let cmd = tokens[0];
            let arg = tokens[1];
            let sender = tokens[2];
            Some((cmd, Some(arg), sender, None))
        }
        "joined" => {
            let cmd = tokens[0];
            let arg = tokens[1];
            let sender = tokens[1];
            let message = tokens[3];

            Some((cmd, Some(arg), sender, Some(message)))
        }
        "list" => {
            let cmd = tokens[0];
            let arg = tokens[1];
            let sender = tokens[2];

            Some((cmd, Some(arg), sender, None))
        }
        "rooms" => {
            let cmd = tokens[0];
            let sender = tokens[1];
            let message;
            if tokens.len() < 3 {
                message = None;
            } else {
                message = Some(tokens[2]);
            }

            Some((cmd, None, sender, message))
        }
        "users" => {
            let cmd = tokens[0];
            let sender = tokens[1];
            let message;
            if tokens.len() < 3 {
                message = None;
            } else {
                message = Some(tokens[2]);
            }

            Some((cmd, None, sender, message))
        }
        "name" => {
            let cmd = tokens[0];
            let arg = tokens[1];
            let sender = tokens[2];

            Some((cmd, Some(arg), sender, None))
        }
        "changedname" => {
            let cmd = tokens[0];
            let arg = tokens[1];
            let sender = tokens[2];
            let message = tokens[3];

            Some((cmd, Some(arg), sender, Some(message)))
        }
        "sendto" => {
            let cmd = tokens[0];
            let arg = tokens[1];
            let sender = tokens[2];
            let message = tokens[3];

            Some((cmd, Some(arg), sender, Some(message)))
        }
        "roommessage" => {
            let cmd = tokens[0];
            let arg = tokens[1];
            let sender = tokens[2];
            let message = tokens[3];

            Some((cmd, Some(arg), sender, Some(message)))
        }

        _ => None,
    }
}
