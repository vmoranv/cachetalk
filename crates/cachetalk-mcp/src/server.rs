use std::io::{BufRead, Write};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Map, Value};

use cachetalk_hal::{CacheTopology, MicroArchHal, MockHal};

use crate::tools::{
    CacheOccupancyTransport, Channel, ChannelConfig, ChannelError, CovertMode, CovertTransport,
    LoopbackTransport,
};
use crate::VERSION;

pub const PROTOCOL_VERSION: &str = "2024-11-05";

pub fn tool_definitions() -> Value {
    json!([
        {
            "name": "send_covert_msg",
            "description": "Transmit a covert message through the configured in-process transport using the fsk mode.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "minLength": 1,
                        "description": "UTF-8 message to transmit."
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["prime_probe", "fsk", "port_contention"],
                        "default": "fsk",
                        "description": "Covert channel mode."
                    },
                    "priority": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 10,
                        "default": 5,
                        "description": "Scheduling priority reported for the transmission."
                    }
                },
                "required": ["message"],
                "additionalProperties": false
            }
        },
        {
            "name": "listen_covert_msg",
            "description": "Wait for a covert message on the configured in-process transport.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "timeout_ms": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 600000,
                        "default": 5000,
                        "description": "Maximum time to wait in milliseconds."
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["prime_probe", "fsk", "port_contention"],
                        "default": "fsk",
                        "description": "Covert channel mode."
                    }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "get_channel_status",
            "description": "Report the covert channel configuration and pending transmissions.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }
    ])
}

#[derive(Clone, Debug)]
pub struct McpServer<T: CovertTransport = LoopbackTransport> {
    channel: Channel<T>,
}

impl McpServer<LoopbackTransport> {
    pub fn loopback() -> Self {
        Self::new(LoopbackTransport::new())
    }
}

impl McpServer<CacheOccupancyTransport> {
    pub fn cache_occupancy() -> Self {
        let hal: Arc<dyn MicroArchHal> = Arc::new(MockHal::new(CacheTopology::unknown()));
        let transport = CacheOccupancyTransport::new(hal, 64, 64, 100)
            .expect("default cache occupancy configuration is valid");
        Self::new(transport)
    }
}

impl<T: CovertTransport> McpServer<T> {
    pub fn new(transport: T) -> Self {
        Self {
            channel: Channel::new(transport),
        }
    }

    pub fn with_config(transport: T, config: ChannelConfig) -> Self {
        Self {
            channel: Channel::with_config(transport, config),
        }
    }

    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => return Some(error_response(Value::Null, -32700, "parse error").to_string()),
        };
        if let Some(batch) = value.as_array() {
            if batch.is_empty() {
                return Some(error_response(Value::Null, -32600, "invalid request").to_string());
            }
            let responses: Vec<Value> = batch
                .iter()
                .filter_map(|request| self.handle_request(request))
                .collect();
            if responses.is_empty() {
                None
            } else {
                Some(Value::Array(responses).to_string())
            }
        } else {
            self.handle_request(&value)
                .map(|response| response.to_string())
        }
    }

    pub fn serve<R: BufRead, W: Write>(&mut self, mut reader: R, mut writer: W) {
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if let Some(response) = self.handle_line(&line) {
                        if writeln!(writer, "{response}").is_err() {
                            break;
                        }
                        if writer.flush().is_err() {
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }
    }

    fn handle_request(&mut self, request: &Value) -> Option<Value> {
        let object = match request.as_object() {
            Some(object) => object,
            None => return Some(error_response(Value::Null, -32600, "invalid request")),
        };
        let id = object.get("id").cloned();
        let method = match object.get("method").and_then(Value::as_str) {
            Some(method) => method,
            None => return Some(error_response(Value::Null, -32600, "invalid request")),
        };
        if id.is_none() || method == "notifications/initialized" {
            return None;
        }
        let id = id.unwrap();
        let params = object.get("params").cloned().unwrap_or(Value::Null);
        match method {
            "initialize" => Some(success_response(
                id,
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {"tools": {}},
                    "serverInfo": {
                        "name": "cachetalk-mcp",
                        "version": VERSION
                    }
                }),
            )),
            "ping" => Some(success_response(id, json!({}))),
            "tools/list" => Some(success_response(id, json!({"tools": tool_definitions()}))),
            "tools/call" => Some(self.handle_tool_call(id, &params)),
            _ => Some(error_response(
                id,
                -32601,
                &format!("method not found: {method}"),
            )),
        }
    }

    fn handle_tool_call(&mut self, id: Value, params: &Value) -> Value {
        let object = match params.as_object() {
            Some(object) => object,
            None => return error_response(id, -32602, "invalid params: expected an object"),
        };
        let name = match object.get("name").and_then(Value::as_str) {
            Some(name) => name,
            None => return error_response(id, -32602, "invalid params: missing tool name"),
        };
        let arguments = match object.get("arguments") {
            None => Map::new(),
            Some(value) => match value.as_object() {
                Some(arguments) => arguments.clone(),
                None if value.is_null() => Map::new(),
                None => {
                    return error_response(
                        id,
                        -32602,
                        "invalid params: arguments must be an object",
                    )
                }
            },
        };
        match name {
            "send_covert_msg" => match self.send_tool(&arguments) {
                Ok(result) => success_response(id, result),
                Err(message) => error_response(id, -32602, &message),
            },
            "listen_covert_msg" => match self.listen_tool(&arguments) {
                Ok(result) => success_response(id, result),
                Err(message) => error_response(id, -32602, &message),
            },
            "get_channel_status" => success_response(id, self.status_tool()),
            _ => error_response(id, -32602, &format!("unknown tool: {name}")),
        }
    }

    fn send_tool(&mut self, arguments: &Map<String, Value>) -> Result<Value, String> {
        let message = arguments
            .get("message")
            .and_then(Value::as_str)
            .ok_or("missing or invalid 'message'")?;
        if message.is_empty() {
            return Err("'message' must not be empty".to_string());
        }
        let mode = parse_mode(arguments)?;
        let priority = parse_priority(arguments)?;
        let result = match self.channel.send_message(message, mode, priority) {
            Ok(report) => tool_result(
                json!({
                    "sent": true,
                    "mode": report.mode.as_str(),
                    "message_bytes": report.message_bytes,
                    "frames": report.frames,
                    "frame_bytes": report.frame_bytes,
                    "bits": report.bits,
                    "samples": report.samples,
                    "priority": report.priority
                }),
                false,
            ),
            Err(error) => tool_result(
                json!({
                    "sent": false,
                    "mode": mode.as_str(),
                    "error": error.to_string()
                }),
                true,
            ),
        };
        Ok(result)
    }

    fn listen_tool(&mut self, arguments: &Map<String, Value>) -> Result<Value, String> {
        let mode = parse_mode(arguments)?;
        let timeout_ms = parse_timeout(arguments)?;
        let result = match self
            .channel
            .receive_message(mode, Duration::from_millis(timeout_ms))
        {
            Ok(report) => tool_result(
                json!({
                    "received": true,
                    "message": report.message,
                    "ber": report.ber,
                    "mode": report.mode.as_str(),
                    "frames": report.frames,
                    "frame_bytes": report.frame_bytes
                }),
                false,
            ),
            Err(ChannelError::Timeout) => tool_result(
                json!({
                    "received": false,
                    "mode": mode.as_str(),
                    "timeout_ms": timeout_ms
                }),
                false,
            ),
            Err(error) => tool_result(
                json!({
                    "received": false,
                    "mode": mode.as_str(),
                    "error": error.to_string()
                }),
                true,
            ),
        };
        Ok(result)
    }

    fn status_tool(&self) -> Value {
        let config = self.channel.config();
        tool_result(
            json!({
                "version": VERSION,
                "transport": self.channel.transport().name(),
                "mode": config.default_mode.as_str(),
                "available_modes": CovertMode::ALL
                    .iter()
                    .map(|mode| mode.as_str())
                    .collect::<Vec<_>>(),
                "fsk": {
                    "sample_rate": config.fsk.sample_rate,
                    "mark_freq": config.fsk.mark_freq,
                    "space_freq": config.fsk.space_freq,
                    "samples_per_bit": config.fsk.samples_per_bit
                },
                "max_payload": config.max_payload,
                "ecc": config.use_ecc,
                "queued_transmissions": self.channel.pending_transmissions()
            }),
            false,
        )
    }
}

fn parse_mode(arguments: &Map<String, Value>) -> Result<CovertMode, String> {
    match arguments.get("mode") {
        None => Ok(CovertMode::default()),
        Some(value) if value.is_null() => Ok(CovertMode::default()),
        Some(value) => {
            let name = value.as_str().ok_or("'mode' must be a string")?;
            CovertMode::parse(name).ok_or_else(|| format!("unknown mode: {name}"))
        }
    }
}

fn parse_priority(arguments: &Map<String, Value>) -> Result<u8, String> {
    match arguments.get("priority") {
        None => Ok(5),
        Some(value) if value.is_null() => Ok(5),
        Some(value) => {
            let priority = value.as_u64().ok_or("'priority' must be an integer")?;
            if !(1..=10).contains(&priority) {
                return Err("'priority' must be between 1 and 10".to_string());
            }
            Ok(priority as u8)
        }
    }
}

fn parse_timeout(arguments: &Map<String, Value>) -> Result<u64, String> {
    match arguments.get("timeout_ms") {
        None => Ok(5000),
        Some(value) if value.is_null() => Ok(5000),
        Some(value) => {
            let timeout = value.as_u64().ok_or("'timeout_ms' must be an integer")?;
            if !(1..=600_000).contains(&timeout) {
                return Err("'timeout_ms' must be between 1 and 600000".to_string());
            }
            Ok(timeout)
        }
    }
}

fn tool_result(payload: Value, is_error: bool) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": payload.to_string()
        }],
        "isError": is_error
    })
}

fn success_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(id: u64, method: &str, params: Value) -> String {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        })
        .to_string()
    }

    fn parse(response: String) -> Value {
        serde_json::from_str(&response).expect("response is valid JSON")
    }

    fn call<T: CovertTransport>(
        server: &mut McpServer<T>,
        id: u64,
        name: &str,
        arguments: Value,
    ) -> Value {
        let line = request(
            id,
            "tools/call",
            json!({"name": name, "arguments": arguments}),
        );
        parse(server.handle_line(&line).expect("tool call response"))
    }

    fn payload(response: &Value) -> Value {
        let text = response["result"]["content"][0]["text"]
            .as_str()
            .expect("tool payload text");
        serde_json::from_str(text).expect("tool payload is valid JSON")
    }

    #[test]
    fn initialize_returns_protocol_and_server_info() {
        let mut server = McpServer::loopback();
        let line = request(
            1,
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "0"}
            }),
        );
        let response = parse(server.handle_line(&line).expect("initialize response"));
        assert_eq!(response["jsonrpc"].as_str(), Some("2.0"));
        assert_eq!(response["id"].as_u64(), Some(1));
        assert_eq!(
            response["result"]["protocolVersion"].as_str(),
            Some(PROTOCOL_VERSION)
        );
        assert_eq!(
            response["result"]["serverInfo"]["name"].as_str(),
            Some("cachetalk-mcp")
        );
        assert_eq!(
            response["result"]["serverInfo"]["version"].as_str(),
            Some(VERSION)
        );
        assert!(response["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn tools_list_exposes_three_tools() {
        let mut server = McpServer::loopback();
        let line = request(2, "tools/list", json!({}));
        let response = parse(server.handle_line(&line).expect("tools list response"));
        let tools = response["result"]["tools"].as_array().expect("tools array");
        assert_eq!(tools.len(), 3);
        let names: Vec<&str> = tools
            .iter()
            .map(|tool| tool["name"].as_str().expect("tool name"))
            .collect();
        assert_eq!(
            names,
            vec!["send_covert_msg", "listen_covert_msg", "get_channel_status"]
        );
    }

    #[test]
    fn tools_call_send_and_listen_round_trip() {
        let transport = LoopbackTransport::new();
        let mut sender = McpServer::new(transport.clone());
        let mut receiver = McpServer::new(transport);

        let response = call(
            &mut sender,
            1,
            "send_covert_msg",
            json!({"message": "hello", "priority": 7}),
        );
        assert_eq!(response["result"]["isError"].as_bool(), Some(false));
        let sent = payload(&response);
        assert_eq!(sent["sent"].as_bool(), Some(true));
        assert_eq!(sent["mode"].as_str(), Some("fsk"));
        assert_eq!(sent["priority"].as_u64(), Some(7));
        assert_eq!(sent["message_bytes"].as_u64(), Some(5));
        assert_eq!(sent["frames"].as_u64(), Some(1));
        assert!(sent["samples"].as_u64().unwrap() > 0);

        let response = call(
            &mut receiver,
            2,
            "listen_covert_msg",
            json!({"timeout_ms": 1000}),
        );
        assert_eq!(response["result"]["isError"].as_bool(), Some(false));
        let received = payload(&response);
        assert_eq!(received["received"].as_bool(), Some(true));
        assert_eq!(received["message"].as_str(), Some("hello"));
        assert_eq!(received["mode"].as_str(), Some("fsk"));
        assert_eq!(received["ber"].as_f64(), Some(0.0));
        assert_eq!(received["frames"].as_u64(), Some(1));
    }

    #[test]
    fn cache_occupancy_server_round_trips_and_reports_transport() {
        let transport = CacheOccupancyTransport::new(
            Arc::new(MockHal::new(CacheTopology::unknown())),
            8,
            64,
            100,
        )
        .unwrap();
        let mut sender = McpServer::new(transport.clone());
        let mut receiver = McpServer::new(transport);

        let response = call(&mut sender, 1, "send_covert_msg", json!({"message": "phy"}));
        assert_eq!(response["result"]["isError"].as_bool(), Some(false));
        let sent = payload(&response);
        assert_eq!(sent["sent"].as_bool(), Some(true));

        let response = call(
            &mut receiver,
            2,
            "listen_covert_msg",
            json!({"timeout_ms": 1000}),
        );
        assert_eq!(response["result"]["isError"].as_bool(), Some(false));
        let received = payload(&response);
        assert_eq!(received["received"].as_bool(), Some(true));
        assert_eq!(received["message"].as_str(), Some("phy"));

        let response = call(&mut sender, 3, "get_channel_status", json!({}));
        let status = payload(&response);
        assert_eq!(status["transport"].as_str(), Some("cache_occupancy"));
    }

    #[test]
    fn listen_timeout_returns_not_received() {
        let mut server = McpServer::loopback();
        let response = call(
            &mut server,
            1,
            "listen_covert_msg",
            json!({"timeout_ms": 1}),
        );
        assert_eq!(response["result"]["isError"].as_bool(), Some(false));
        let payload = payload(&response);
        assert_eq!(payload["received"].as_bool(), Some(false));
        assert_eq!(payload["timeout_ms"].as_u64(), Some(1));
        assert_eq!(payload["mode"].as_str(), Some("fsk"));
    }

    #[test]
    fn get_channel_status_reports_configuration() {
        let mut server = McpServer::loopback();
        let response = call(&mut server, 1, "get_channel_status", json!({}));
        assert_eq!(response["result"]["isError"].as_bool(), Some(false));
        let payload = payload(&response);
        assert_eq!(payload["version"].as_str(), Some(VERSION));
        assert_eq!(payload["transport"].as_str(), Some("loopback"));
        assert_eq!(payload["mode"].as_str(), Some("fsk"));
        assert_eq!(payload["available_modes"].as_array().unwrap().len(), 3);
        assert_eq!(payload["fsk"]["sample_rate"].as_u64(), Some(1000));
        assert_eq!(payload["fsk"]["mark_freq"].as_u64(), Some(100));
        assert_eq!(payload["fsk"]["space_freq"].as_u64(), Some(50));
        assert_eq!(payload["fsk"]["samples_per_bit"].as_u64(), Some(100));
        assert_eq!(payload["max_payload"].as_u64(), Some(1024));
        assert_eq!(payload["ecc"].as_bool(), Some(true));
        assert_eq!(payload["queued_transmissions"].as_u64(), Some(0));
    }

    #[test]
    fn unsupported_mode_reports_tool_error() {
        let mut server = McpServer::loopback();
        let response = call(
            &mut server,
            1,
            "send_covert_msg",
            json!({"message": "hello", "mode": "prime_probe"}),
        );
        assert_eq!(response["result"]["isError"].as_bool(), Some(true));
        let payload = payload(&response);
        assert_eq!(payload["sent"].as_bool(), Some(false));
        assert_eq!(payload["mode"].as_str(), Some("prime_probe"));
        assert!(payload["error"]
            .as_str()
            .unwrap()
            .contains("calibrated hardware transport"));
    }

    #[test]
    fn invalid_tool_arguments_return_invalid_params() {
        let mut server = McpServer::loopback();
        let cases = [
            ("send_covert_msg", json!({"priority": 5})),
            ("send_covert_msg", json!({"message": ""})),
            ("send_covert_msg", json!({"message": "x", "priority": 0})),
            ("send_covert_msg", json!({"message": "x", "priority": 11})),
            (
                "send_covert_msg",
                json!({"message": "x", "mode": "unknown"}),
            ),
            ("listen_covert_msg", json!({"timeout_ms": 0})),
            ("listen_covert_msg", json!({"timeout_ms": 600001})),
            ("get_channel_status", json!([])),
        ];
        for (index, (name, arguments)) in cases.into_iter().enumerate() {
            let response = call(&mut server, index as u64, name, arguments);
            assert_eq!(
                response["error"]["code"].as_i64(),
                Some(-32602),
                "case {index}: {response}"
            );
        }
    }

    #[test]
    fn unknown_tool_returns_invalid_params() {
        let mut server = McpServer::loopback();
        let response = call(&mut server, 1, "missing_tool", json!({}));
        assert_eq!(response["error"]["code"].as_i64(), Some(-32602));
    }

    #[test]
    fn unknown_method_returns_method_not_found() {
        let mut server = McpServer::loopback();
        let line = request(1, "missing/method", json!({}));
        let response = parse(server.handle_line(&line).expect("error response"));
        assert_eq!(response["error"]["code"].as_i64(), Some(-32601));
    }

    #[test]
    fn invalid_json_returns_parse_error() {
        let mut server = McpServer::loopback();
        let response = parse(
            server
                .handle_line("{not json")
                .expect("parse error response"),
        );
        assert_eq!(response["id"], Value::Null);
        assert_eq!(response["error"]["code"].as_i64(), Some(-32700));
    }

    #[test]
    fn notification_produces_no_response() {
        let mut server = McpServer::loopback();
        assert!(server
            .handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
            .is_none());
    }

    #[test]
    fn batch_requests_return_array() {
        let mut server = McpServer::loopback();
        let batch = json!([
            {"jsonrpc": "2.0", "id": 1, "method": "ping"},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
            {"jsonrpc": "2.0", "method": "notifications/initialized"}
        ])
        .to_string();
        let response = parse(server.handle_line(&batch).expect("batch response"));
        let responses = response.as_array().expect("batch array");
        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0]["id"].as_u64(), Some(1));
        assert_eq!(responses[1]["id"].as_u64(), Some(2));
    }

    #[test]
    fn empty_batch_returns_invalid_request() {
        let mut server = McpServer::loopback();
        let response = parse(server.handle_line("[]").expect("error response"));
        assert_eq!(response["error"]["code"].as_i64(), Some(-32600));
    }

    #[test]
    fn serve_reads_lines_and_writes_responses() {
        let input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n";
        let mut output = Vec::new();
        let mut server = McpServer::loopback();
        server.serve(&input[..], &mut output);
        let response: Value = serde_json::from_slice(&output).expect("response JSON");
        assert_eq!(response["id"].as_u64(), Some(1));
        assert!(response["result"].is_object());
    }
}
