//! A health check on the server's own port: `GET /health` (plain HTTP, no WebSocket) answers
//! `ok`, with the rooms and games, for a container's health check or a proxy.

use std::io::{Read, Write};
use std::net::TcpStream;

/// Whether the connection is a health check, from its first bytes (without reading them).
pub fn is_check(stream: &TcpStream) -> bool {
    let mut start = [0u8; 12];
    matches!(stream.peek(&mut start), Ok(n) if start[..n].starts_with(b"GET /health"))
}

/// Answers a health check and closes the connection.
pub fn answer(mut stream: TcpStream, rooms: usize, games: usize) {
    // The request is read first: a socket closed with bytes unread is reset, and the client
    // would lose the answer.
    let mut request = Vec::new();
    let mut buf = [0u8; 512];
    while !request.windows(4).any(|w| w == b"\r\n\r\n") && request.len() < 8192 {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => request.extend_from_slice(&buf[..n]),
        }
    }
    let body = format!("ok rooms={rooms} games={games}\n");
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.flush();
}
