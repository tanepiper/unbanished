//! Hosting a multiplayer game on this computer (not in the original): when a host opens a room
//! on a server at this computer's own address and none is running, the game starts one on a
//! thread, open to its network. Desktop only: a browser cannot listen.
//!
//! Deviation: not in the original, which has no multiplayer.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

/// The port of a server address on this computer (`ws://127.0.0.1:7780`, `ws://localhost:7780`),
/// if it is one.
pub fn local_port(address: &str) -> Option<u16> {
    let rest = address.strip_prefix("ws://")?;
    let (host, port) = rest.trim_end_matches('/').rsplit_once(':')?;
    matches!(host, "127.0.0.1" | "localhost" | "[::1]")
        .then(|| port.parse().ok())
        .flatten()
}

/// Starts a server on `port` for every network of this computer, unless the port is taken
/// (a server is already running there). The address players on this computer's network enter,
/// when it started one.
pub fn start(port: u16) -> Option<String> {
    let config = unbanished_server::Config {
        address: format!("0.0.0.0:{port}"),
        ..Default::default()
    };
    match unbanished_server::spawn(config) {
        Ok(_) => Some(match network_address() {
            Some(ip) => format!("ws://{ip}:{port}"),
            None => format!("ws://<this computer's address>:{port}"),
        }),
        Err(e) => {
            log::info!("multiplayer: port {port} is taken ({e}); using the server there");
            None
        }
    }
}

/// This computer's address on its network. A UDP socket "connected" to an outside address
/// sends nothing; its own address is the one the network sees.
fn network_address() -> Option<IpAddr> {
    let socket = UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0))).ok()?;
    socket.connect(("192.0.2.1", 9)).ok()?;
    Some(socket.local_addr().ok()?.ip()).filter(|ip| !ip.is_unspecified())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_server_address_on_this_computer() {
        assert_eq!(local_port("ws://127.0.0.1:7780"), Some(7780));
        assert_eq!(local_port("ws://localhost:9000/"), Some(9000));
        assert_eq!(local_port("ws://192.168.1.20:7780"), None);
        assert_eq!(local_port("wss://127.0.0.1:7780"), None);
        assert_eq!(local_port("ws://127.0.0.1"), None);
    }

    #[test]
    fn a_host_starts_a_server_once() {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let address = start(port).expect("the port was free");
        assert!(address.ends_with(&format!(":{port}")), "{address}");
        let mut c = unbanished_net::Connection::connect(&format!("ws://127.0.0.1:{port}"));
        c.send(unbanished_net::ToRelay::Hello(unbanished_net::Hello::new(
            "test", 1, 2,
        )));
        let mut welcomed = false;
        for _ in 0..1000 {
            if c.poll()
                .iter()
                .any(|m| matches!(m, unbanished_net::FromRelay::Welcome { .. }))
            {
                welcomed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(welcomed, "{:?}", c.state());
        assert_eq!(start(port), None, "a second server is not started");
    }
}
