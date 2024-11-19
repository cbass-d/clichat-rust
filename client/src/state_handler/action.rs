pub enum Action {
    Connect { addr: String },
    Disconnect,
    Send { data: String },
    Quit,
}
