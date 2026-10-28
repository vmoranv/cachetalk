use cachetalk_mcp::{
    bit_error_rate, Channel, ChannelConfig, ChannelError, CovertMode, LoopbackTransport, McpServer,
    MessageAssembler, MessageEncoder, SemanticError, CHUNK_FLAG_FIRST, CHUNK_HEADER_LEN,
};
use cachetalk_modem::Frame;
use serde_json::{json, Value};
use std::time::Duration;

fn channel_pair() -> (Channel<LoopbackTransport>, Channel<LoopbackTransport>) {
    let transport = LoopbackTransport::new();
    let config = ChannelConfig {
        max_payload: 64,
        use_ecc: false,
        ..Default::default()
    };
    (
        Channel::with_config(transport.clone(), config.clone()),
        Channel::with_config(transport, config),
    )
}

fn parse_response(server: &mut McpServer<LoopbackTransport>, line: &str) -> Value {
    let response = server.handle_line(line).expect("expected a response");
    serde_json::from_str(&response).expect("response is valid JSON")
}

#[test]
fn channel_round_trip_fragments_long_messages() {
    let (mut sender, mut receiver) = channel_pair();
    let message = "x".repeat(150);

    let sent = sender
        .send_message(&message, CovertMode::Fsk, 7)
        .expect("send succeeds");
    assert_eq!(sent.mode, CovertMode::Fsk);
    assert_eq!(sent.message_bytes, 150);
    assert_eq!(sent.frames, 3);
    assert_eq!(sent.priority, 7);
    assert_eq!(sent.frame_bytes, 207);
    assert_eq!(sent.bits, 3312);
    assert_eq!(sent.samples, 331_200);

    let received = receiver
        .receive_message(CovertMode::Fsk, Duration::from_millis(500))
        .expect("receive succeeds");
    assert_eq!(received.mode, CovertMode::Fsk);
    assert_eq!(received.message, message);
    assert_eq!(received.frames, 3);
    assert_eq!(received.frame_bytes, 207);
    assert_eq!(received.ber, 0.0);
}

#[test]
fn encoder_and_assembler_handle_multi_chunk_messages() {
    let encoder = MessageEncoder::new(32);
    assert_eq!(encoder.chunk_capacity(), 27);
    let message = "abcdefghij".repeat(10);
    let chunks = encoder.encode(message.as_bytes()).expect("encode succeeds");
    assert_eq!(chunks.len(), 4);
    assert_eq!(chunks[0][0], CHUNK_FLAG_FIRST);
    assert_eq!(chunks[1][0], 0);
    let declared = u32::from_le_bytes(chunks[0][1..5].try_into().unwrap());
    assert_eq!(declared, 100);
    assert_eq!(chunks[3].len(), CHUNK_HEADER_LEN + 19);

    let mut assembler = MessageAssembler::new();
    for (sequence, chunk) in chunks.iter().enumerate() {
        let frame = Frame {
            sequence: sequence as u8,
            payload: chunk.clone(),
            ecc: false,
        };
        let assembled = assembler.push(&frame).expect("push succeeds");
        if sequence + 1 == chunks.len() {
            assert_eq!(assembled.as_deref(), Some(message.as_str()));
        } else {
            assert!(assembled.is_none());
        }
    }

    let mut out_of_order = MessageAssembler::new();
    let first = Frame {
        sequence: 0,
        payload: chunks[0].clone(),
        ecc: false,
    };
    assert!(out_of_order
        .push(&first)
        .expect("first push succeeds")
        .is_none());
    let third = Frame {
        sequence: 2,
        payload: chunks[2].clone(),
        ecc: false,
    };
    let error = out_of_order
        .push(&third)
        .expect_err("sequence gap is rejected");
    assert!(matches!(
        error,
        SemanticError::SequenceMismatch {
            expected: 1,
            received: 2
        }
    ));
}

#[test]
fn bit_error_rate_matches_bit_differences() {
    assert_eq!(bit_error_rate(&[0, 1, 1, 0], &[0, 1, 1, 0]), 0.0);
    assert_eq!(bit_error_rate(&[0, 0, 1, 1], &[0, 1, 0, 1]), 0.5);
    assert_eq!(bit_error_rate(&[1, 0], &[0, 1]), 1.0);
    assert_eq!(bit_error_rate(&[], &[]), 0.0);
    assert_eq!(bit_error_rate(&[1, 1], &[1, 1, 1, 1]), 0.5);
}

#[test]
fn channel_rejects_modes_without_a_hardware_transport() {
    let mut channel = Channel::new(LoopbackTransport::new());
    let send_error = channel
        .send_message("hello", CovertMode::PrimeProbe, 5)
        .expect_err("prime probe needs a hardware transport");
    assert!(matches!(
        send_error,
        ChannelError::UnsupportedMode(CovertMode::PrimeProbe)
    ));

    let receive_error = channel
        .receive_message(CovertMode::PortContention, Duration::from_millis(10))
        .expect_err("port contention needs a hardware transport");
    assert!(matches!(
        receive_error,
        ChannelError::UnsupportedMode(CovertMode::PortContention)
    ));
}

#[test]
fn empty_transport_times_out() {
    let mut channel = Channel::new(LoopbackTransport::new());
    let error = channel
        .receive_message(CovertMode::Fsk, Duration::from_millis(10))
        .expect_err("empty loopback must time out");
    assert!(matches!(error, ChannelError::Timeout));
}

#[test]
fn server_reports_protocol_and_tool_errors() {
    let mut server = McpServer::loopback();
    assert!(server.handle_line("").is_none());
    assert!(server.handle_line("   \n\t").is_none());
    assert!(server
        .handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
        .is_none());

    let parse_error = parse_response(&mut server, "{not json");
    assert_eq!(parse_error["id"], Value::Null);
    assert_eq!(parse_error["error"]["code"], json!(-32700));
    assert_eq!(parse_error["error"]["message"], json!("parse error"));

    let invalid_batch = parse_response(&mut server, "[]");
    assert_eq!(invalid_batch["error"]["code"], json!(-32600));
    assert_eq!(invalid_batch["error"]["message"], json!("invalid request"));

    let unknown_tool = parse_response(
        &mut server,
        r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"nope","arguments":{}}}"#,
    );
    assert_eq!(unknown_tool["id"], json!(9));
    assert_eq!(unknown_tool["error"]["code"], json!(-32602));
    assert_eq!(
        unknown_tool["error"]["message"],
        json!("unknown tool: nope")
    );
}
