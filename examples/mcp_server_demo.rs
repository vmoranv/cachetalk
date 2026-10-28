use cachetalk_mcp::{CovertTransport, McpServer};
use serde_json::{json, Value};

fn call<T: CovertTransport>(
    server: &mut McpServer<T>,
    id: u64,
    method: &str,
    params: Value,
) -> Value {
    let request = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    })
    .to_string();
    let response = server.handle_line(&request).expect("server should respond");
    serde_json::from_str(&response).expect("response should be valid JSON")
}

fn main() {
    let mut server = McpServer::cache_occupancy();

    let initialize = call(
        &mut server,
        1,
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "example", "version": "0.1.0"},
        }),
    );
    println!("initialize: {initialize}");

    let tools = call(&mut server, 2, "tools/list", json!({}));
    println!("tools/list: {tools}");

    let sent = call(
        &mut server,
        3,
        "tools/call",
        json!({
            "name": "send_covert_msg",
            "arguments": {"message": "hello from cachetalk", "mode": "fsk"},
        }),
    );
    println!("send_covert_msg: {sent}");

    let received = call(
        &mut server,
        4,
        "tools/call",
        json!({
            "name": "listen_covert_msg",
            "arguments": {"timeout_ms": 1000},
        }),
    );
    println!("listen_covert_msg: {received}");

    let status = call(
        &mut server,
        5,
        "tools/call",
        json!({
            "name": "get_channel_status",
            "arguments": {},
        }),
    );
    println!("get_channel_status: {status}");
}
