pub fn pack_message(
    cmd: &str,
    arg: Option<&str>,
    sender: &str,
    id: u64,
    message: Option<&str>,
) -> String {
    let mut packed = String::from("!#");
    packed.push_str(cmd);

    if let Some(arg) = arg {
        packed.push('#');
        packed.push_str(arg);
    }

    packed.push('#');
    packed.push_str(sender);
    packed.push('#');
    packed.push_str(&id.to_string());
    if let Some(message) = message {
        packed.push('#');
        packed.push_str(message);
    }
    packed.push_str("#!");

    packed
}

pub fn unpack_message(message: &str) -> Option<(&str, Option<&str>, &str, &str, Option<&str>)> {
    if !message.starts_with("!") || !message.ends_with("!") {
        return None;
    }

    // Remove starting and ending !s
    let content = &message[1..message.len() - 1];
    let tokens: Vec<&str> = content.split('#').filter(|s| !s.is_empty()).collect();
    let cmd = tokens[0];

    match cmd {
        "register" | "join" | "leave" | "list" | "name" | "create" | "created" => {
            let arg = tokens[1];
            let sender = tokens[2];
            let id = tokens[3];

            Some((cmd, Some(arg), sender, id, None))
        }
        "registered" | "joined" | "left" | "privmsg" => {
            let arg = tokens[1];
            let sender = tokens[2];
            let id = tokens[3];
            let message = tokens[4];

            Some((cmd, Some(arg), sender, id, Some(message)))
        }
        "rooms" | "users" | "message" => {
            let sender = tokens[1];
            let id = tokens[2];
            let message;
            if tokens.len() < 4 {
                message = None;
            } else {
                message = Some(tokens[3]);
            }

            Some((cmd, None, sender, id, message))
        }
        "changedname" | "sendto" | "roommessage" => {
            let arg = tokens[1];
            let sender = tokens[2];
            let id = tokens[3];
            let message = tokens[4];

            Some((cmd, Some(arg), sender, id, Some(message)))
        }
        _ => None,
    }
}
