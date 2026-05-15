//! Anvil CRDT Engine — subprocess adapter binary.
//! Reads JSON-RPC style commands from stdin, writes responses to stdout.
//! This bridges the Python benchmark harness to the Rust engine.

mod engine;

use engine::EngineHost;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", content = "args")]
enum Command {
    OpenPeer {
        peer_id: String,
    },
    ApplySchema {
        peer_id: String,
        stmts: Vec<String>,
    },
    Execute {
        peer_id: String,
        sql: String,
        params: Vec<serde_json::Value>,
    },
    Sync {
        peer_a: String,
        peer_b: String,
    },
    SnapshotHash {
        peer_id: String,
    },
    SnapshotState {
        peer_id: String,
    },
    Close,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status")]
enum Response {
    #[serde(rename = "ok")]
    Ok { result: serde_json::Value },
    #[serde(rename = "error")]
    Error { message: String },
}

fn ok(v: impl Serialize) -> Response {
    Response::Ok {
        result: serde_json::to_value(v).unwrap_or(serde_json::Value::Null),
    }
}

fn err(e: impl std::fmt::Display) -> Response {
    Response::Error {
        message: e.to_string(),
    }
}

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());

    let mut host = EngineHost::new();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                let resp = err(e);
                let _ = writeln!(out, "{}", serde_json::to_string(&resp).unwrap());
                let _ = out.flush();
                break;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<Command>(&line) {
            Err(e) => err(format!("parse error: {e}")),
            Ok(cmd) => match cmd {
                Command::OpenPeer { peer_id } => {
                    host.open_peer(&peer_id);
                    ok(serde_json::Value::Null)
                }
                Command::ApplySchema { peer_id, stmts } => {
                    match host.apply_schema(&peer_id, &stmts) {
                        Ok(()) => ok(serde_json::Value::Null),
                        Err(e) => err(e),
                    }
                }
                Command::Execute {
                    peer_id,
                    sql,
                    params,
                } => match host.execute(&peer_id, &sql, &params) {
                    Ok(result) => ok(result),
                    Err(e) => err(e),
                },
                Command::Sync { peer_a, peer_b } => match host.sync(&peer_a, &peer_b) {
                    Ok(()) => ok(serde_json::Value::Null),
                    Err(e) => err(e),
                },
                Command::SnapshotHash { peer_id } => match host.snapshot_hash(&peer_id) {
                    Ok(hash) => ok(hash),
                    Err(e) => err(e),
                },
                Command::SnapshotState { peer_id } => match host.snapshot_state(&peer_id) {
                    Ok(state) => ok(state),
                    Err(e) => err(e),
                },
                Command::Close => {
                    host.close();
                    ok(serde_json::Value::Null)
                }
            },
        };

        let line_out = serde_json::to_string(&response)
            .unwrap_or_else(|e| serde_json::to_string(&err(e)).unwrap());
        let _ = writeln!(out, "{line_out}");
        let _ = out.flush();
    }
}
