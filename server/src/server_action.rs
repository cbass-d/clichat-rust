#[derive(Clone)]
pub enum ServerRequest {
    Register {
        id: u64,
        name: String,
    },
    JoinRoom {
        room: String,
        id: u64,
    },
    LeaveRoom {
        room: String,
        id: u64,
    },
    SendTo {
        room: String,
        content: String,
        id: u64,
    },
    List {
        opt: String,
        id: u64,
    },
    DropSession {
        id: u64,
    },
    CreateRoom {
        room: String,
        id: u64,
    },
    PrivMsg {
        user: String,
        content: String,
        id: u64,
    },
    ChangeName {
        new_name: String,
        id: u64,
    },
}

pub enum ServerResponse {
    Registered { name: String },
    Joined { room: String },
    Listing { opt: String, content: String },
    LeftRoom { room: String },
    CreatedRoom { room: String },
    Messaged { user: String, content: String },
    NameChanged { new_name: String, old_name: String },
    Failed { error: String },
}
