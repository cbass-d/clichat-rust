use common::message::{Message, MessageBody, MessageHeader, MessageType};

#[test]
fn serialize() {
    let header = MessageHeader {
        message_type: MessageType::Join,
        sender_id: 1,
    };

    let body = MessageBody {
        arg: Some(String::from("main")),
        content: None,
    };

    let message = Message { header, body };
    let bytes = message.to_bytes();
    assert_eq!(
        bytes,
        vec![2, 3, 74, 111, 110, 1, 1, 4, 109, 97, 105, 110, 0]
    );
}

#[test]
fn deserialize() {
    let bytes = vec![2, 3, 74, 111, 110, 1, 1, 4, 109, 97, 105, 110, 0];

    let header = MessageHeader {
        message_type: MessageType::Join,
        sender_id: 1,
    };

    let body = MessageBody {
        arg: Some(String::from("main")),
        content: None,
    };

    let message_orig = Message { header, body };
    let message_new = Message::from_bytes(bytes).unwrap();

    assert_eq!(message_new, message_orig);
}
