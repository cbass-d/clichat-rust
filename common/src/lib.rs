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

    match tokens.len() {
        2 => {
            let cmd = tokens[0];
            let sender = tokens[1];

            Some((cmd, None, sender, None))
        }
        3 => {
            let cmd = tokens[0];
            let arg;
            let message;
            let sender;
            if cmd == "rooms" {
                arg = None;
                sender = tokens[1];
                message = Some(tokens[2]);
            } else {
                arg = Some(tokens[1]);
                sender = tokens[2];
                message = None;
            }

            Some((cmd, arg, sender, message))
        }
        4 => {
            let cmd = tokens[0];
            let arg = Some(tokens[1]);
            let sender = tokens[2];
            let message = tokens[3];

            Some((cmd, arg, sender, Some(message)))
        }
        _ => None,
    }
}
