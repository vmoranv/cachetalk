mod common;

use std::time::Duration;

use common::{bench, bench_batched};

use cachetalk::mcp::{
    Channel, ChannelConfig, CovertMode, LoopbackTransport, McpServer, MessageAssembler,
    MessageEncoder,
};
use cachetalk::modem::{Frame, FskConfig};

fn fast_channel_config() -> ChannelConfig {
    ChannelConfig {
        fsk: FskConfig {
            sample_rate: 8_000,
            mark_freq: 2_000,
            space_freq: 1_000,
            samples_per_bit: 8,
        },
        ..ChannelConfig::default()
    }
}

fn bench_channel() {
    let config = fast_channel_config();

    bench_batched(
        "channel/send_receive_hello",
        || Channel::with_config(LoopbackTransport::new(), config.clone()),
        |mut channel| {
            let sent = channel
                .send_message("hello", CovertMode::Fsk, 0)
                .expect("send");
            let received = channel
                .receive_message(CovertMode::Fsk, Duration::from_millis(100))
                .expect("receive");
            (sent, received)
        },
    );
}

fn bench_handle_line() {
    let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
    let tools_list = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
    let status = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_channel_status","arguments":{}}}"#;

    for (name, line) in [
        ("initialize", initialize),
        ("tools_list", tools_list),
        ("get_channel_status", status),
    ] {
        let mut server = McpServer::loopback();
        bench(&format!("handle_line/{name}"), || server.handle_line(line));
    }
}

fn bench_semantic() {
    let encoder = MessageEncoder::new(1024);
    let message = [0x5Au8; 256];
    let frames: Vec<Frame> = encoder
        .encode(&message)
        .expect("encode")
        .into_iter()
        .enumerate()
        .map(|(index, payload)| Frame {
            sequence: index as u8,
            payload,
            ecc: true,
        })
        .collect();

    bench("semantic/encode_256_bytes", || {
        encoder.encode(&message).expect("encode")
    });
    bench("semantic/assemble_256_bytes", || {
        let mut assembler = MessageAssembler::new();
        for frame in &frames {
            assembler.push(frame).expect("push");
        }
        assembler
    });
}

fn main() {
    bench_channel();
    bench_handle_line();
    bench_semantic();
}
