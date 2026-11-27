//! End-to-end check of `Session` against a fake PvPGN chat gateway.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

use bnetchat::chat::ChatState;
use bnetchat::protocol::ServerEvent;
use bnetchat::session::{Credentials, Session};

/// Accept one client, run the login handshake, then send `lines` in pieces
/// that split protocol lines across TCP writes.
fn fake_server(lines: &'static str) -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let handle = thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(sock.try_clone().unwrap());
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte).unwrap();
        assert_eq!(byte[0], 3);

        let mut read_line = || {
            let mut l = String::new();
            reader.read_line(&mut l).unwrap();
            l.trim_end().to_string()
        };
        sock.write_all(b"Username: ").unwrap();
        let user = read_line();
        sock.write_all(b"Password: ").unwrap();
        let _pass = read_line();
        sock.write_all(format!("2010 NAME {user}\r\n").as_bytes())
            .unwrap();
        assert_eq!(read_line(), "/join w3");
        sock.write_all(b"1007 CHANNEL \"W3\"\r\n1001 USER Alice 0010 [W3XP]\r\n")
            .unwrap();

        // Split mid-line to exercise buffering in the reader.
        let (a, b) = lines.split_at(lines.len() / 2);
        sock.write_all(a.as_bytes()).unwrap();
        sock.flush().unwrap();
        thread::sleep(Duration::from_millis(100));
        sock.write_all(b.as_bytes()).unwrap();

        // Chat line, then the logout sent by Session::close.
        let sent = vec![read_line(), read_line()];
        drop(sock);
        sent
    });
    (addr, handle)
}

#[test]
fn logs_in_streams_events_and_reports_disconnect() {
    let (addr, server) = fake_server(
        "1002 JOIN Bob 0000 [W3XP]\r\n1005 TALK Bob 0000 \"hello tester\"\r\n1018 INFO \"W3 topic: be nice\"\r\n",
    );
    let cred = Credentials {
        server: addr,
        user: "tester".into(),
        password: "pw".into(),
    };

    let (tx, rx) = channel();
    let mut session = Session::connect(&cred, move |e| {
        let _ = tx.send(e);
    })
    .expect("login");
    session.send("hi all").unwrap();
    thread::sleep(Duration::from_millis(300));
    session.close();

    let mut chat = ChatState::new(session.username());
    let mut events = Vec::new();
    while let Ok(e) = rx.recv_timeout(Duration::from_secs(5)) {
        let done = matches!(e, ServerEvent::Disconnected { .. });
        events.push(e.clone());
        chat.apply(e);
        if done {
            break;
        }
    }

    assert_eq!(
        server.join().unwrap(),
        vec!["hi all".to_string(), "/quit".to_string()]
    );
    assert_eq!(
        events.first(),
        Some(&ServerEvent::Channel { name: "W3".into() })
    );
    assert!(events.contains(&ServerEvent::Talk {
        from: "Bob".into(),
        text: "hello tester".into()
    }));
    assert!(matches!(
        events.last(),
        Some(ServerEvent::Disconnected { .. })
    ));
    assert_eq!(chat.channel.as_deref(), Some("W3"));
    assert_eq!(chat.topic.as_deref(), Some("be nice"));
    assert_eq!(chat.users.len(), 2);
    assert!(!chat.connected);
    assert!(
        chat.messages.iter().any(|m| chat.is_highlight(m)),
        "mention of our name is highlighted"
    );
}
