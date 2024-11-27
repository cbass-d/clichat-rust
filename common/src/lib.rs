pub fn pack_message(
    origin: &str,
    cmd: &str,
    arg: Option<&str>,
    sender: &str,
    message: Option<&str>,
) -> String {
    let mut packed = format!("!{}", origin);
    packed.push('#');
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

pub fn unpack_message(message: &str) -> Option<(&str, &str, Option<&str>, &str, Option<&str>)> {
    if !message.starts_with("!") || !message.ends_with("!") {
        return None;
    }

    // Remove starting and ending !
    let content = &message[1..message.len() - 1];
    let tokens: Vec<&str> = content.split('#').filter(|s| !s.is_empty()).collect();

    match tokens.len() {
        4 => {
            let addr = tokens[0];
            let cmd = tokens[1];
            let arg = Some(tokens[2]);
            let sender = tokens[3];

            Some((addr, cmd, arg, sender, None))
        }
        5 => {
            let addr = tokens[0];
            let cmd = tokens[1];
            let arg = Some(tokens[2]);
            let sender = tokens[3];
            let message = tokens[4];

            Some((addr, cmd, arg, sender, Some(message)))
        }
        6 => {
            let addr = tokens[0];
            let cmd = tokens[1];
            let arg = Some(tokens[2]);
            let sender = tokens[3];
            let message = tokens[4];

            Some((addr, cmd, arg, sender, Some(message)))
        }
        _ => None,
    }
}
