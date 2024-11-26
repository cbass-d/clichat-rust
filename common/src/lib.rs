pub fn pack_message(origin: &str, room: Option<&str>, sender: &str, message: &str) -> String {
    let mut packed = format!("!{}", origin);

    if let Some(room) = room {
        packed.push('#');
        packed.push_str(&room);
    }

    packed.push('#');
    packed.push_str(&sender);
    packed.push('#');
    packed.push_str(&message);
    packed.push_str("#!");

    packed
}

pub fn unpack_message(message: &str) -> Option<(&str, Option<&str>, &str, &str)> {
    if !message.starts_with("!") || !message.ends_with("!") {
        return None;
    }

    // Remove starting and ending !
    let content = &message[1..message.len() - 1];
    let tokens: Vec<&str> = content.split('#').collect();

    match tokens.len() {
        4 => {
            let addr = tokens[0];
            let room_option = None;
            let sender = tokens[1];
            let message = tokens[2];

            Some((addr, room_option, sender, message))
        }
        5 => {
            let addr = tokens[0];
            let room_option = Some(tokens[1]);
            let sender = tokens[2];
            let message = tokens[3];

            Some((addr, room_option, sender, message))
        }
        _ => None,
    }
}
