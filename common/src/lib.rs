#[derive(Debug, PartialEq)]
pub struct Message {
    pub cmd: String,
    pub arg: Option<String>,
    pub sender: String,
    pub id: u64,
    pub content: Option<String>,
}

pub fn pack_message(message: Message) -> String {
    let mut packed = String::from("!#");
    packed.push_str(&message.cmd);

    if let Some(arg) = message.arg {
        packed.push('#');
        packed.push_str(&arg);
    }

    packed.push('#');
    packed.push_str(&message.sender);
    packed.push('#');
    packed.push_str(&message.id.to_string());
    if let Some(content) = message.content {
        packed.push('#');
        packed.push_str(&content);
    }
    packed.push_str("#!");

    packed
}

pub fn unpack_message(message: &str) -> Option<Message> {
    if !message.starts_with("!") || !message.ends_with("!") {
        return None;
    }

    // Remove starting and ending !s
    let content = &message[1..message.len() - 1];
    let tokens: Vec<&str> = content.split('#').filter(|s| !s.is_empty()).collect();
    let cmd = tokens[0];

    match cmd {
        "register" | "join" | "joined" | "leave" | "leftroom" | "list" | "changename"
        | "create" | "createdroom" => {
            if tokens.len() != 4 {
                return None;
            }
            let arg = tokens[1].to_string();
            let sender = tokens[2].to_string();
            let id = tokens[3].parse::<u64>().unwrap();

            Some(Message {
                cmd: cmd.to_string(),
                arg: Some(arg),
                sender,
                id,
                content: None,
            })
        }
        "failed" | "registered" | "privmsg" | "outgoingmsg" | "changedname" | "sendto"
        | "roommessage" => {
            if tokens.len() != 5 {
                return None;
            }
            let arg = tokens[1].to_string();
            let sender = tokens[2].to_string();
            let id = tokens[3].parse::<u64>().unwrap();
            let content = tokens[4].to_string();

            Some(Message {
                cmd: cmd.to_string(),
                arg: Some(arg),
                sender,
                id,
                content: Some(content),
            })
        }
        "rooms" | "allrooms" | "users" | "incomingmsg" => {
            if tokens.len() != 4 && tokens.len() != 3 {
                return None;
            }
            let sender = tokens[1].to_string();
            let id = tokens[2].parse::<u64>().unwrap();
            let content;
            if tokens.len() < 4 {
                content = None;
            } else {
                content = Some(tokens[3].to_string());
            }

            Some(Message {
                cmd: cmd.to_string(),
                arg: None,
                sender,
                id,
                content,
            })
        }
        _ => None,
    }
}
