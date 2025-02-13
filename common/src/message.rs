use anyhow::{anyhow, Result};
use postcard::{from_bytes, to_stdvec};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MessageType {
    Register,
    Registered,
    Join,
    Joined,
    Leave,
    LeftRoom,
    List,
    ChangeName,
    ChangedName,
    Create,
    CreatedRoom,
    PrivMsg,
    IncomingMsg,
    OutgoingMsg,
    SendTo,
    MessagedRoom,
    RoomMessage,
    UserRooms,
    AllRooms,
    Users,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageHeader {
    pub message_type: MessageType,
    pub sender_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageBody {
    pub arg: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub header: MessageHeader,
    pub body: MessageBody,
}

fn check_message(message: &Message) -> Result<()> {
    match message.header.message_type {
        MessageType::Join
        | MessageType::ChangeName
        | MessageType::Leave
        | MessageType::List
        | MessageType::Create
        | MessageType::Register
        | MessageType::Joined
        | MessageType::LeftRoom
        | MessageType::CreatedRoom => {
            if message.body.arg.is_none() {
                return Err(anyhow!("Argument required"));
            }

            if message.body.content.is_some() {
                return Err(anyhow!("Unecessary content provided"));
            }
        }

        MessageType::SendTo
        | MessageType::PrivMsg
        | MessageType::RoomMessage
        | MessageType::Registered
        | MessageType::Failed
        | MessageType::ChangedName
        | MessageType::MessagedRoom
        | MessageType::OutgoingMsg => {
            if message.body.arg.is_none() {
                return Err(anyhow!("Argument required"));
            }

            if message.body.content.is_none() {
                return Err(anyhow!("Content required"));
            }
        }

        MessageType::IncomingMsg
        | MessageType::AllRooms
        | MessageType::UserRooms
        | MessageType::Users => {
            if message.body.arg.is_some() {
                return Err(anyhow!("Uncessary argument provided"));
            }

            if message.body.content.is_none() {
                return Err(anyhow!("Content required"));
            }
        }
    }

    Ok(())
}

impl Message {
    pub fn build(
        message_type: MessageType,
        sender_id: u64,
        arg: Option<String>,
        content: Option<String>,
    ) -> Result<Self> {
        let message = Message {
            header: MessageHeader {
                message_type,
                sender_id,
            },
            body: MessageBody { arg, content },
        };

        check_message(&message)?;

        Ok(message)
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let message = from_bytes(&bytes)?;

        check_message(&message)?;

        Ok(message)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        to_stdvec(self).unwrap()
    }
}
