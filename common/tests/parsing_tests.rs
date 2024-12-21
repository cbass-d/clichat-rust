use common;

#[test]
fn packing() {
    // Includes commands that are parsed the same:
    // * register, * join, * joined, * leave, * leftroom
    // * list, * changename, * create, * createdroom
    let first_set = common::Message {
        cmd: String::from("register"),
        arg: Some(String::from("jon")),
        sender: String::from("new-user"),
        id: 1,
        content: None,
    };
    let packed = common::pack_message(first_set);
    assert_eq!(packed, "!#register#jon#new-user#1#!");

    // * failed, * registered, * privmsg, * outgoingmsg, * changedname
    // * sendto, * roommesage
    let second_set = common::Message {
        cmd: String::from("roommessage"),
        arg: Some(String::from("main")),
        sender: String::from("jon"),
        id: 1,
        content: Some(String::from("hello main!")),
    };

    let packed = common::pack_message(second_set);
    assert_eq!(packed, "!#roommessage#main#jon#1#hello main!#!");

    // * rooms, * allrooms, * users, * incomingmsg
    // With content attached
    let third_set_content = common::Message {
        cmd: String::from("rooms"),
        arg: None,
        sender: String::from("server"),
        id: 0,
        content: Some(String::from("main,other-room,tech")),
    };

    let packed = common::pack_message(third_set_content);
    assert_eq!(packed, "!#rooms#server#0#main,other-room,tech#!");

    // With no content attached
    let third_set_no_content = common::Message {
        cmd: String::from("users"),
        arg: None,
        sender: String::from("server"),
        id: 0,
        content: Some(String::from("jon,jack,peter")),
    };

    let packed = common::pack_message(third_set_no_content);
    assert_eq!(packed, "!#users#server#0#jon,jack,peter#!");
}

#[test]
fn unpacking() {
    let first_set = "!#register#jon#new-user#1#!";
    let unpacked = common::unpack_message(first_set).unwrap();
    assert_eq!(
        unpacked,
        common::Message {
            cmd: String::from("register"),
            arg: Some(String::from("jon")),
            sender: String::from("new-user"),
            id: 1,
            content: None,
        }
    );

    let second_set = "!#roommessage#main#jon#1#hello main!#!";
    let unpacked = common::unpack_message(second_set).unwrap();
    assert_eq!(
        unpacked,
        common::Message {
            cmd: String::from("roommessage"),
            arg: Some(String::from("main")),
            sender: String::from("jon"),
            id: 1,
            content: Some(String::from("hello main!")),
        }
    );

    let third_set_content = "!#rooms#server#0#main,other-room,tech#!";
    let unpacked = common::unpack_message(third_set_content).unwrap();
    assert_eq!(
        unpacked,
        common::Message {
            cmd: String::from("rooms"),
            arg: None,
            sender: String::from("server"),
            id: 0,
            content: Some(String::from("main,other-room,tech")),
        }
    );

    let third_set_no_content = "!#users#server#0#jon,jack,peter#!";
    let unpacked = common::unpack_message(third_set_no_content).unwrap();
    assert_eq!(
        unpacked,
        common::Message {
            cmd: String::from("users"),
            arg: None,
            sender: String::from("server"),
            id: 0,
            content: Some(String::from("jon,jack,peter")),
        }
    );
}
