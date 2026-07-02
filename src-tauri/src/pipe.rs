//! Named-pipe control API (M2.2, roadmap §2.4b).
//!
//! An external broker process connects to a local named pipe and speaks
//! newline-delimited JSON. The transport is request/response only (no server
//! push): a broker that wants live output polls `readOutput` with an advancing
//! byte offset. This keeps each connection a simple blocking read→dispatch→
//! write loop and avoids a second writer thread.
//!
//! Security (ADR-008): the pipe is user-scoped (Windows default named-pipe
//! DACL); the first message must authenticate with a token that terminal-f
//! writes to a user-readable file at startup. Every capability the API exposes
//! (injectPrompt, readOutput) still passes the same backend gates as the UI —
//! the pipe is a transport, not a bypass.
//!
//! This module owns the transport + auth loop and a *pure* `dispatch` that is
//! unit-tested with a stub handler. The AppState-specific method routing lives
//! in `commands::handle_pipe_method`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::sync::Arc;

/// Per-connection state carried across requests.
pub struct ConnState {
    pub authed: bool,
    /// Client-supplied name (audit/label); "?" until provided.
    pub client: String,
}

impl ConnState {
    pub fn new() -> Self {
        Self {
            authed: false,
            client: "?".into(),
        }
    }
}

impl Default for ConnState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Deserialize)]
struct Req {
    #[serde(default)]
    id: Value,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Serialize)]
struct Resp {
    id: Value,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn ok(id: Value, result: Value) -> String {
    serde_json::to_string(&Resp {
        id,
        ok: true,
        result: Some(result),
        error: None,
    })
    .unwrap_or_else(|_| "{\"ok\":false,\"error\":\"serialize\"}".into())
}

fn err(id: Value, message: String) -> String {
    serde_json::to_string(&Resp {
        id,
        ok: false,
        result: None,
        error: Some(message),
    })
    .unwrap_or_else(|_| "{\"ok\":false,\"error\":\"serialize\"}".into())
}

/// Handle one request line. `auth` and `ping` are handled here; all other
/// methods require prior auth and are delegated to `method_handler`.
pub fn dispatch<F>(token: &str, st: &mut ConnState, line: &str, method_handler: &F) -> String
where
    F: Fn(&str, &Value, &mut ConnState) -> Result<Value, String>,
{
    let req: Req = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => return err(Value::Null, format!("invalid request: {e}")),
    };
    match req.method.as_str() {
        "auth" => {
            let supplied = req.params.get("token").and_then(|t| t.as_str()).unwrap_or("");
            if !supplied.is_empty() && supplied == token {
                st.authed = true;
                if let Some(name) = req.params.get("client").and_then(|c| c.as_str()) {
                    st.client = name.to_string();
                }
                ok(req.id, serde_json::json!({ "authed": true }))
            } else {
                st.authed = false;
                err(req.id, "authentication failed".into())
            }
        }
        "ping" if st.authed => ok(req.id, serde_json::json!({ "pong": true })),
        _ if !st.authed => err(req.id, "not authenticated (send auth first)".into()),
        _ => match method_handler(&req.method, &req.params, st) {
            Ok(result) => ok(req.id, result),
            Err(e) => err(req.id, e),
        },
    }
}

/// Serve one connection: read lines, dispatch, write responses. Blocks until
/// the client disconnects. Generic over the method handler so the transport
/// is testable with a stub.
pub fn serve_connection<S, F>(stream: S, token: &str, method_handler: &F)
where
    S: std::io::Read + std::io::Write,
    F: Fn(&str, &Value, &mut ConnState) -> Result<Value, String>,
{
    let mut st = ConnState::new();
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break, // EOF or error
            Ok(_) => {
                let trimmed = line.trim_end();
                if trimmed.is_empty() {
                    continue;
                }
                let resp = dispatch(token, &mut st, trimmed, method_handler);
                let w = reader.get_mut();
                if w.write_all(resp.as_bytes()).is_err() || w.write_all(b"\n").is_err() {
                    break;
                }
                let _ = w.flush();
            }
        }
    }
}

/// Info written to disk so brokers can find and authenticate to the pipe.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ControlApiInfo {
    pub pipe_name: String,
    pub token: String,
}

/// Start the pipe listener and serve each connection on its own thread.
/// `handler` is invoked per authenticated request line.
pub fn start_server<F>(pipe_name: String, token: String, handler: Arc<F>) -> std::io::Result<()>
where
    F: Fn(&str, &Value, &mut ConnState) -> Result<Value, String> + Send + Sync + 'static,
{
    use interprocess::local_socket::prelude::*;
    use interprocess::local_socket::{GenericNamespaced, ListenerOptions};

    let name = pipe_name
        .clone()
        .to_ns_name::<GenericNamespaced>()
        .map_err(std::io::Error::other)?;
    let listener = ListenerOptions::new().name(name).create_sync()?;

    std::thread::Builder::new()
        .name("control-pipe".into())
        .spawn(move || {
            for conn in listener.incoming() {
                let Ok(conn) = conn else { continue };
                let token = token.clone();
                let handler = Arc::clone(&handler);
                std::thread::Builder::new()
                    .name("control-pipe-conn".into())
                    .spawn(move || {
                        serve_connection(conn, &token, &*handler);
                    })
                    .ok();
            }
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stub(method: &str, _params: &Value, _st: &mut ConnState) -> Result<Value, String> {
        match method {
            "listPanes" => Ok(serde_json::json!({ "panes": [] })),
            other => Err(format!("unknown method: {other}")),
        }
    }

    #[test]
    fn auth_required_before_methods() {
        let mut st = ConnState::new();
        let r = dispatch("secret", &mut st, r#"{"id":1,"method":"listPanes"}"#, &stub);
        assert!(r.contains("not authenticated"));
        assert!(!st.authed);
    }

    #[test]
    fn auth_success_and_failure() {
        let mut st = ConnState::new();
        let bad = dispatch(
            "secret",
            &mut st,
            r#"{"id":1,"method":"auth","params":{"token":"wrong"}}"#,
            &stub,
        );
        assert!(bad.contains("authentication failed"));
        assert!(!st.authed);

        let good = dispatch(
            "secret",
            &mut st,
            r#"{"id":2,"method":"auth","params":{"token":"secret","client":"broker-x"}}"#,
            &stub,
        );
        assert!(good.contains("\"ok\":true"));
        assert!(st.authed);
        assert_eq!(st.client, "broker-x");
    }

    #[test]
    fn method_routing_after_auth() {
        let mut st = ConnState::new();
        st.authed = true;
        let ok = dispatch("secret", &mut st, r#"{"id":3,"method":"listPanes"}"#, &stub);
        assert!(ok.contains("\"ok\":true"));
        assert!(ok.contains("panes"));
        let unknown = dispatch("secret", &mut st, r#"{"id":4,"method":"nope"}"#, &stub);
        assert!(unknown.contains("unknown method: nope"));
    }

    #[test]
    fn ping_requires_auth() {
        let mut st = ConnState::new();
        let denied = dispatch("secret", &mut st, r#"{"id":5,"method":"ping"}"#, &stub);
        assert!(denied.contains("not authenticated"));
        st.authed = true;
        let pong = dispatch("secret", &mut st, r#"{"id":6,"method":"ping"}"#, &stub);
        assert!(pong.contains("pong"));
    }

    #[test]
    fn malformed_json_is_rejected() {
        let mut st = ConnState::new();
        let r = dispatch("secret", &mut st, "not json", &stub);
        assert!(r.contains("invalid request"));
    }
}
