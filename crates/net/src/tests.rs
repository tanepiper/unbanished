use super::memory::{MemoryConnection, MemoryRelay};
use super::*;

fn hello() -> Hello {
    Hello::new("Un-Banished 0.4.0-test", 0x1234, 0x5678)
}

/// A connection that has said Hello and been welcomed.
fn greeted(relay: &MemoryRelay, hello: Hello) -> MemoryConnection {
    let mut c = relay.connect();
    c.send(ToRelay::Hello(hello));
    assert_eq!(c.poll(), [FromRelay::Welcome { protocol: PROTOCOL }]);
    c
}

#[test]
fn values_round_trip() {
    let values = [
        ToRelay::Hello(hello()),
        ToRelay::ListRooms,
        ToRelay::Host {
            room: None,
            password: Some("pw".into()),
        },
        ToRelay::Join {
            room: "ABCDEF".into(),
            password: None,
        },
        ToRelay::Leave,
        ToRelay::Send {
            to: Some(2),
            data: vec![1, 2, 3],
        },
        ToRelay::Ping(7),
    ];
    for v in values {
        assert_eq!(decode::<ToRelay>(&encode(&v)).unwrap(), v);
    }
    let big = FromRelay::Data {
        from: 1,
        data: vec![7; 1 << 20],
    };
    let bytes = encode(&big);
    assert!(bytes.len() < (1 << 20) + 16, "{}", bytes.len());
    assert_eq!(decode::<FromRelay>(&bytes).unwrap(), big);
}

#[test]
fn a_cut_off_message_is_an_error() {
    let bytes = encode(&ToRelay::Hello(hello()));
    for n in 0..bytes.len() {
        assert!(
            decode::<ToRelay>(&bytes[..n]).is_err(),
            "{n} of {} bytes",
            bytes.len()
        );
    }
    assert!(decode::<ToRelay>(&[0xff, 0xff, 0xff]).is_err());
}

#[test]
fn a_game_of_another_protocol_is_refused_with_the_reason() {
    let relay = MemoryRelay::new(1);
    let mut c = relay.connect();
    let mut old = hello();
    old.protocol = PROTOCOL + 1;
    c.send(ToRelay::Hello(old));
    let FromRelay::Error { message } = &c.poll()[0] else {
        panic!("not refused");
    };
    assert!(message.contains("protocol"), "{message}");
    assert!(matches!(c.state(), State::Closed(_)));
}

#[test]
fn a_game_says_hello_first() {
    let relay = MemoryRelay::new(1);
    let mut c = relay.connect();
    c.send(ToRelay::ListRooms);
    assert!(matches!(&c.poll()[0], FromRelay::Error { .. }));
    assert!(matches!(c.state(), State::Closed(_)));
}

#[test]
fn players_list_host_join_and_pass_data() {
    let relay = MemoryRelay::new(1);
    let mut host = greeted(&relay, hello());
    host.send(ToRelay::Host {
        room: Some("TESTER".into()),
        password: None,
    });
    assert_eq!(
        host.poll(),
        [FromRelay::Hosted {
            room: "TESTER".into(),
            keeps: false,
            kept: None
        }]
    );
    let mut a = greeted(&relay, hello());
    a.send(ToRelay::ListRooms);
    let room = RoomInfo {
        code: "TESTER".into(),
        players: 1,
        password: false,
    };
    assert_eq!(a.poll(), [FromRelay::Rooms(vec![room])]);
    a.send(ToRelay::Join {
        room: "TESTER".into(),
        password: None,
    });
    let joined = FromRelay::Joined {
        room: "TESTER".into(),
        player: 1,
    };
    assert_eq!(a.poll(), [joined]);
    assert_eq!(host.poll(), [FromRelay::PeerJoined { player: 1 }]);
    host.send(ToRelay::Send {
        to: None,
        data: vec![9],
    });
    assert_eq!(
        a.poll(),
        [FromRelay::Data {
            from: 0,
            data: vec![9]
        }]
    );
    a.send(ToRelay::Ping(3));
    assert_eq!(a.poll(), [FromRelay::Pong(3)]);
    a.send(ToRelay::Leave);
    assert_eq!(host.poll(), [FromRelay::PeerLeft { player: 1 }]);
    drop(host);
    a.send(ToRelay::ListRooms);
    assert_eq!(
        a.poll(),
        [FromRelay::Rooms(Vec::new())],
        "an empty room closes"
    );
}

#[test]
fn a_room_checks_its_password() {
    let relay = MemoryRelay::new(1);
    let mut host = greeted(&relay, hello());
    host.send(ToRelay::Host {
        room: Some("LOCKED".into()),
        password: Some("secret".into()),
    });
    host.poll();
    let mut a = greeted(&relay, hello());
    for password in [None, Some("wrong".to_string())] {
        a.send(ToRelay::Join {
            room: "LOCKED".into(),
            password,
        });
        assert!(matches!(&a.poll()[0], FromRelay::Error { .. }));
    }
    a.send(ToRelay::Join {
        room: "LOCKED".into(),
        password: Some("secret".into()),
    });
    assert!(matches!(&a.poll()[0], FromRelay::Joined { player: 1, .. }));
}

#[test]
fn a_game_unlike_the_host_is_refused_with_the_reason() {
    let relay = MemoryRelay::new(1);
    let mut host = greeted(&relay, hello());
    host.send(ToRelay::Host {
        room: Some("ROOMAB".into()),
        password: None,
    });
    host.poll();
    let other = [
        (
            "version",
            Hello::new("Un-Banished 0.5.0-other", 0x1234, 0x5678),
        ),
        ("data", Hello::new("Un-Banished 0.4.0-test", 0x9999, 0x5678)),
        ("mods", Hello::new("Un-Banished 0.4.0-test", 0x1234, 0x9999)),
    ];
    for (what, h) in other {
        let mut c = greeted(&relay, h);
        c.send(ToRelay::Join {
            room: "ROOMAB".into(),
            password: None,
        });
        let FromRelay::Error { message } = &c.poll()[0] else {
            panic!("{what}: not refused");
        };
        assert!(message.contains(what), "{what}: {message}");
    }
    assert!(host.poll().is_empty(), "the host heard of nobody");
}

#[test]
fn the_server_asks_the_majority_for_a_save() {
    let relay = MemoryRelay::new(1);
    let mut host = greeted(&relay, hello());
    host.send(ToRelay::Host {
        room: Some("CHECKS".into()),
        password: None,
    });
    host.poll();
    let mut players = vec![host];
    for _ in 0..2 {
        let mut c = greeted(&relay, hello());
        c.send(ToRelay::Join {
            room: "CHECKS".into(),
            password: None,
        });
        players.push(c);
    }
    for p in players.iter_mut() {
        p.poll();
    }
    // Players 0 and 2 agree; player 1 differs.
    for (p, crc) in [(0, 7), (1, 8), (2, 7)] {
        players[p].send(ToRelay::Digest {
            epoch: 0,
            update: 600,
            crc,
        });
    }
    let differ = FromRelay::Checked {
        update: 600,
        agree: false,
    };
    let wanted = FromRelay::SaveWanted { update: 600 };
    assert_eq!(players[0].poll(), [differ.clone(), wanted]);
    assert_eq!(players[1].poll(), [differ.clone()]);
    assert_eq!(players[2].poll(), [differ]);
    // After the resync (epoch 1) they agree: nobody is asked.
    for p in 0..3 {
        players[p].send(ToRelay::Digest {
            epoch: 1,
            update: 1200,
            crc: 9,
        });
    }
    let agree = FromRelay::Checked {
        update: 1200,
        agree: true,
    };
    assert!(players.iter_mut().all(|p| p.poll() == [agree.clone()]));
}

#[test]
fn a_server_that_keeps_rooms_gives_the_town_back() {
    let relay = MemoryRelay::with_store(1, Box::new(super::store::MemoryStore::default()));
    let mut host = greeted(&relay, hello());
    host.send(ToRelay::Host {
        room: Some("KEPT".into()),
        password: None,
    });
    let hosted = FromRelay::Hosted {
        room: "KEPT".into(),
        keeps: true,
        kept: None,
    };
    assert_eq!(host.poll(), [hosted]);
    // A player who is not the host cannot keep the town.
    let mut guest = greeted(&relay, hello());
    guest.send(ToRelay::Join {
        room: "KEPT".into(),
        password: None,
    });
    guest.send(ToRelay::Keep {
        update: 5,
        bytes: vec![9],
    });
    host.send(ToRelay::Keep {
        update: 600,
        bytes: vec![1, 2],
    });
    drop(guest);
    drop(host);
    // The room closed; a host who opens it again finds its town.
    let mut again = greeted(&relay, hello());
    again.send(ToRelay::Host {
        room: Some("KEPT".into()),
        password: None,
    });
    let hosted = FromRelay::Hosted {
        room: "KEPT".into(),
        keeps: true,
        kept: Some(600),
    };
    assert_eq!(again.poll(), [hosted]);
    again.send(ToRelay::Fetch);
    let kept = FromRelay::Kept {
        update: 600,
        bytes: vec![1, 2],
    };
    assert_eq!(again.poll(), [kept]);
}

#[test]
fn a_watched_room_keeps_the_servers_game() {
    use super::rooms::Relay;
    let mut relay = Relay::new(1);
    for c in 0..3 {
        relay.connect(c);
        relay.handle(c, ToRelay::Hello(hello()));
    }
    relay.handle(
        0,
        ToRelay::Host {
            room: Some("WATCH".into()),
            password: None,
        },
    );
    for c in 1..3 {
        relay.handle(
            c,
            ToRelay::Join {
                room: "WATCH".into(),
                password: None,
            },
        );
    }
    relay.watch("WATCH");
    // The players' digests first (player 2 differs), the server's last.
    for (c, crc) in [(0, 7), (1, 7), (2, 8)] {
        let reply = relay.handle(
            c,
            ToRelay::Digest {
                epoch: 0,
                update: 480,
                crc,
            },
        );
        assert!(reply.out.is_empty(), "the check waits for the server");
    }
    let out = relay.server_digest("WATCH", 0, 480, 7);
    assert!(out
        .iter()
        .all(|(_, m)| matches!(m, FromRelay::Checked { agree: false, .. })));
    assert_eq!(relay.take_server_saves(), ["WATCH"]);
}
