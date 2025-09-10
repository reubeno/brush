//! Tests for the JSON-RPC interface.

use core::panic;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use assert_fs::TempDir;
use serde_json::Value;

fn read_msg(reader: &mut BufReader<UnixStream>) -> Value {
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}

#[test]
fn alias_rpc_and_command_execution() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let socket_path = tmp.path().join("sock");

    let shell_path = assert_cmd::cargo::cargo_bin("brush");
    let mut child = std::process::Command::new(shell_path)
        .arg("--norc")
        .arg("--noprofile")
        .arg("--rpc-socket")
        .arg(&socket_path)
        .spawn()?;

    // Wait for socket to appear (skip in environments that forbid AF_UNIX sockets)
    let mut appeared = false;
    for _ in 0..200 {
        if socket_path.exists() {
            appeared = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    if !appeared {
        child.kill().ok();
        panic!("socket did not appear");
    }

    let mut stream = UnixStream::connect(&socket_path)?;
    let mut reader = BufReader::new(stream.try_clone()?);

    // Direct alias set
    let req = serde_json::json!({
        "method": "setAlias",
        "id": 1,
        "params": {"name": "foo", "value": "echo"}
    });
    writeln!(stream, "{}", req)?;

    // Expect a response (id=1) and an aliasAdded notification
    let mut saw_resp = false;
    let mut saw_added = false;
    while !saw_resp || !saw_added {
        let msg = read_msg(&mut reader);
        if msg.get("id").is_some() && msg["id"] == 1 && msg.get("result").is_some() {
            saw_resp = true;
        }
        if msg.get("method") == Some(&Value::from("aliasAdded"))
            && msg["params"]["name"] == "foo"
        {
            saw_added = true;
        }
    }

    // Alias via command execution
    let req = serde_json::json!({
        "method": "runCommand",
        "id": 2,
        "params": {"command": "alias bar='baz'"}
    });
    writeln!(stream, "{}", req)?;

    let mut saw_alias = false;
    let mut got_result = false;
    while !saw_alias || !got_result {
        let msg = read_msg(&mut reader);
        if msg.get("method") == Some(&Value::from("aliasAdded"))
            && msg["params"]["name"] == "bar"
        {
            saw_alias = true;
        }
        if msg.get("id").is_some() && msg["id"] == 2 && msg.get("result").is_some() {
            got_result = true;
        }
    }
    assert!(saw_alias, "missing alias_added notification");

    // Invalid command should return error
    let req = serde_json::json!({
        "method": "runCommand",
        "id": 3,
        "params": {"command": "echo \"unterminated"}
    });
    writeln!(stream, "{}", req)?;
    // Read until we get an error response with id 3
    loop {
        let msg = read_msg(&mut reader);
        if msg.get("error").is_some() && msg.get("id") == Some(&Value::from(3)) {
            break;
        }
    }

    child.kill().ok();
    Ok(())
}
