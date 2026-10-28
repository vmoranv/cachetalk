use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl Server {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_cachetalk-mcp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn cachetalk-mcp");
        let stdin = child.stdin.take().expect("server stdin");
        let stdout = child.stdout.take().expect("server stdout");
        Self {
            child,
            stdin: Some(stdin),
            stdout: BufReader::new(stdout),
        }
    }

    fn request(&mut self, request: Value) -> Value {
        self.write_line(&request);
        let mut line = String::new();
        let read = self
            .stdout
            .read_line(&mut line)
            .expect("read response line");
        assert!(read > 0, "server closed stdout before responding");
        serde_json::from_str(&line).expect("response is valid JSON")
    }

    fn notify(&mut self, notification: Value) {
        self.write_line(&notification);
    }

    fn write_line(&mut self, value: &Value) {
        let stdin = self.stdin.as_mut().expect("server stdin is open");
        writeln!(stdin, "{value}").expect("write request");
        stdin.flush().expect("flush request");
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stdin.take();
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

fn call(server: &mut Server, id: u64, method: &str, params: Value) -> Value {
    let response = server.request(json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params
    }));
    assert_eq!(response["jsonrpc"].as_str(), Some("2.0"));
    assert_eq!(response["id"].as_u64(), Some(id));
    response
}

fn tool_payload(response: &Value) -> (Value, bool) {
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("tool payload text");
    let payload = serde_json::from_str(text).expect("tool payload is valid JSON");
    let is_error = response["result"]["isError"]
        .as_bool()
        .expect("isError is a boolean");
    (payload, is_error)
}

#[test]
fn initialize_and_list_tools() {
    let mut server = Server::spawn();

    let response = call(
        &mut server,
        1,
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "stdio-e2e", "version": "0"}
        }),
    );
    assert_eq!(
        response["result"]["protocolVersion"].as_str(),
        Some("2024-11-05")
    );
    assert_eq!(
        response["result"]["serverInfo"]["name"].as_str(),
        Some("cachetalk-mcp")
    );
    assert_eq!(
        response["result"]["serverInfo"]["version"].as_str(),
        Some(env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(response["result"]["capabilities"], json!({"tools": {}}));

    server.notify(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let pong = call(&mut server, 2, "ping", json!({}));
    assert_eq!(pong["result"], json!({}));

    let list = call(&mut server, 3, "tools/list", json!({}));
    let tools = list["result"]["tools"].as_array().expect("tools array");
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert_eq!(names.len(), 3);
    for expected in ["send_covert_msg", "listen_covert_msg", "get_channel_status"] {
        assert!(names.contains(&expected), "missing tool {expected}");
    }
}

#[test]
fn send_listen_and_status_round_trip() {
    let mut server = Server::spawn();

    let send = call(
        &mut server,
        1,
        "tools/call",
        json!({
            "name": "send_covert_msg",
            "arguments": {"message": "hello", "priority": 3}
        }),
    );
    let (payload, is_error) = tool_payload(&send);
    assert!(!is_error);
    assert_eq!(payload["sent"], json!(true));
    assert_eq!(payload["mode"].as_str(), Some("fsk"));
    assert_eq!(payload["message_bytes"].as_u64(), Some(5));
    assert_eq!(payload["frames"].as_u64(), Some(1));
    assert_eq!(payload["frame_bytes"].as_u64(), Some(33));
    assert_eq!(payload["bits"].as_u64(), Some(528));
    assert_eq!(payload["samples"].as_u64(), Some(52_800));
    assert_eq!(payload["priority"].as_u64(), Some(3));

    let listen = call(
        &mut server,
        2,
        "tools/call",
        json!({
            "name": "listen_covert_msg",
            "arguments": {"timeout_ms": 500}
        }),
    );
    let (payload, is_error) = tool_payload(&listen);
    assert!(!is_error);
    assert_eq!(payload["received"], json!(true));
    assert_eq!(payload["message"].as_str(), Some("hello"));
    assert_eq!(payload["ber"].as_f64(), Some(0.0));
    assert_eq!(payload["mode"].as_str(), Some("fsk"));
    assert_eq!(payload["frames"].as_u64(), Some(1));
    assert_eq!(payload["frame_bytes"].as_u64(), Some(33));

    let status = call(
        &mut server,
        3,
        "tools/call",
        json!({"name": "get_channel_status", "arguments": {}}),
    );
    let (payload, is_error) = tool_payload(&status);
    assert!(!is_error);
    assert_eq!(payload["version"].as_str(), Some(env!("CARGO_PKG_VERSION")));
    assert_eq!(payload["transport"].as_str(), Some("cache_occupancy"));
    assert_eq!(payload["mode"].as_str(), Some("fsk"));
    assert_eq!(
        payload["available_modes"],
        json!(["prime_probe", "fsk", "port_contention"])
    );
    assert_eq!(
        payload["fsk"],
        json!({
            "sample_rate": 1000,
            "mark_freq": 100,
            "space_freq": 50,
            "samples_per_bit": 100
        })
    );
    assert_eq!(payload["max_payload"].as_u64(), Some(1024));
    assert_eq!(payload["ecc"], json!(true));
    assert_eq!(payload["queued_transmissions"].as_u64(), Some(0));
}

#[test]
fn reports_tool_and_protocol_errors() {
    let mut server = Server::spawn();

    let unknown = server.request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "does/not/exist"
    }));
    assert_eq!(unknown["error"]["code"].as_i64(), Some(-32601));
    assert_eq!(
        unknown["error"]["message"].as_str(),
        Some("method not found: does/not/exist")
    );

    let empty = call(
        &mut server,
        2,
        "tools/call",
        json!({
            "name": "send_covert_msg",
            "arguments": {"message": ""}
        }),
    );
    assert_eq!(empty["error"]["code"].as_i64(), Some(-32602));
    assert_eq!(
        empty["error"]["message"].as_str(),
        Some("'message' must not be empty")
    );

    let missing = call(
        &mut server,
        3,
        "tools/call",
        json!({"name": "send_covert_msg", "arguments": {}}),
    );
    assert_eq!(missing["error"]["code"].as_i64(), Some(-32602));
    assert_eq!(
        missing["error"]["message"].as_str(),
        Some("missing or invalid 'message'")
    );

    let unknown_tool = call(
        &mut server,
        4,
        "tools/call",
        json!({"name": "no_such_tool", "arguments": {}}),
    );
    assert_eq!(unknown_tool["error"]["code"].as_i64(), Some(-32602));
    assert_eq!(
        unknown_tool["error"]["message"].as_str(),
        Some("unknown tool: no_such_tool")
    );
}

#[test]
fn listen_times_out_and_rejects_unsupported_modes() {
    let mut server = Server::spawn();

    let timeout = call(
        &mut server,
        1,
        "tools/call",
        json!({
            "name": "listen_covert_msg",
            "arguments": {"timeout_ms": 50}
        }),
    );
    let (payload, is_error) = tool_payload(&timeout);
    assert!(!is_error);
    assert_eq!(payload["received"], json!(false));
    assert_eq!(payload["mode"].as_str(), Some("fsk"));
    assert_eq!(payload["timeout_ms"].as_u64(), Some(50));

    let unsupported = call(
        &mut server,
        2,
        "tools/call",
        json!({
            "name": "send_covert_msg",
            "arguments": {"message": "hi", "mode": "prime_probe"}
        }),
    );
    let (payload, is_error) = tool_payload(&unsupported);
    assert!(is_error);
    assert_eq!(payload["sent"], json!(false));
    assert_eq!(payload["mode"].as_str(), Some("prime_probe"));
    assert_eq!(
        payload["error"].as_str(),
        Some(
            "mode 'prime_probe' requires a calibrated hardware transport; \
             only 'fsk' is available in the in-process transport"
        )
    );
}
