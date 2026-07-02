//! Control-API named-pipe transport smoke test: real interprocess listener +
//! client, token auth handshake, and a method round-trip through a stub
//! handler. AppState-backed method routing is covered by unit tests in
//! commands/pipe; this proves the transport + auth loop end to end.

use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{GenericNamespaced, ListenerOptions, Stream};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;
use terminal_f_lib::pipe::{self, ConnState};

fn stub(method: &str, _p: &Value, st: &mut ConnState) -> Result<Value, String> {
    match method {
        "whoami" => Ok(serde_json::json!({ "client": st.client })),
        other => Err(format!("unknown method: {other}")),
    }
}

#[test]
fn pipe_auth_and_method_roundtrip() {
    // Unique pipe name so parallel test runs don't collide.
    let name_str = format!("terminal-f-test-{}.sock", &uuid_like());
    let token = "test-token-123".to_string();

    let name = name_str.clone().to_ns_name::<GenericNamespaced>().unwrap();
    let listener = ListenerOptions::new().name(name).create_sync().unwrap();

    let srv_token = token.clone();
    let handler = Arc::new(stub);
    let server = std::thread::spawn(move || {
        // Serve exactly one connection then return.
        if let Some(Ok(conn)) = listener.incoming().next() {
            pipe::serve_connection(conn, &srv_token, &*handler);
        }
    });

    // Client
    let cname = name_str.to_ns_name::<GenericNamespaced>().unwrap();
    let conn = Stream::connect(cname).expect("client connect");
    let mut reader = BufReader::new(conn);

    let send = |reader: &mut BufReader<Stream>, line: &str| -> String {
        reader.get_mut().write_all(line.as_bytes()).unwrap();
        reader.get_mut().write_all(b"\n").unwrap();
        reader.get_mut().flush().unwrap();
        let mut resp = String::new();
        reader.read_line(&mut resp).unwrap();
        resp
    };

    // method before auth -> rejected
    let denied = send(&mut reader, r#"{"id":1,"method":"whoami"}"#);
    assert!(denied.contains("not authenticated"), "{denied}");

    // wrong token
    let bad = send(
        &mut reader,
        r#"{"id":2,"method":"auth","params":{"token":"nope"}}"#,
    );
    assert!(bad.contains("authentication failed"), "{bad}");

    // correct token + client name
    let good = send(
        &mut reader,
        r#"{"id":3,"method":"auth","params":{"token":"test-token-123","client":"smoke"}}"#,
    );
    assert!(good.contains("\"ok\":true"), "{good}");

    // authed method reflects client name
    let who = send(&mut reader, r#"{"id":4,"method":"whoami"}"#);
    assert!(who.contains("smoke"), "{who}");

    drop(reader); // close client -> server loop ends
    server.join().unwrap();
}

// Cheap unique-ish string without pulling uuid into the test (varies per run
// via process/thread address); good enough to avoid pipe-name collisions.
fn uuid_like() -> String {
    let x = Box::new(0u8);
    format!("{:p}", &*x).replace("0x", "")
}
