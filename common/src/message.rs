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

impl Message {
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        from_bytes(&bytes).unwrap()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        to_stdvec(self).unwrap()
    }
}
