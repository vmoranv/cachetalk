mod common;

use std::hint::black_box;

use cachetalk::modem::{
    arq::{ArqConfig, ArqReceiver, ArqSender},
    bits::bytes_to_bits,
    ecc::{decode_hamming, encode_hamming, encoded_len},
    frame::{decode_frame, encode_frame},
    fsk::{demodulate, modulate, FskConfig},
    manchester::{decode_manchester, encode_manchester},
};
use common::bench;

fn fast_fsk() -> FskConfig {
    FskConfig {
        sample_rate: 8_000,
        mark_freq: 2_000,
        space_freq: 1_000,
        samples_per_bit: 8,
    }
}

fn main() {
    bench_fsk();
    bench_frame();
    bench_manchester();
    bench_hamming();
    bench_arq();
}

fn bench_fsk() {
    let config = fast_fsk();
    let payload = [0xA5u8; 128];
    let bits = bytes_to_bits(&payload);
    let samples = modulate(&bits, config).expect("modulate");

    bench("fsk/modulate_1024_bits", || {
        modulate(black_box(&bits), config).expect("modulate")
    });
    bench("fsk/demodulate_1024_bits", || {
        demodulate(black_box(&samples), bits.len(), config).expect("demodulate")
    });
}

fn bench_frame() {
    let payload = [0x5Au8; 64];
    let plain = encode_frame(7, &payload, false).expect("encode");
    let ecc = encode_frame(7, &payload, true).expect("encode ecc");

    bench("frame/encode_64_bytes", || {
        encode_frame(7, black_box(&payload), false).expect("encode")
    });
    bench("frame/encode_64_bytes_ecc", || {
        encode_frame(7, black_box(&payload), true).expect("encode")
    });
    bench("frame/decode_64_bytes", || {
        decode_frame(black_box(&plain)).expect("decode")
    });
    bench("frame/decode_64_bytes_ecc", || {
        decode_frame(black_box(&ecc)).expect("decode")
    });
}

fn bench_manchester() {
    let data = [0xC3u8; 128];
    let symbols = encode_manchester(&data);

    bench("manchester/encode_128_bytes", || {
        encode_manchester(black_box(&data))
    });
    bench("manchester/decode_2048_symbols", || {
        decode_manchester(black_box(&symbols)).expect("decode")
    });
}

fn bench_hamming() {
    let data = [0x3Cu8; 64];
    let encoded = encode_hamming(&data);
    assert_eq!(encoded.len(), encoded_len(data.len()));

    bench("hamming/encode_64_bytes", || {
        encode_hamming(black_box(&data))
    });
    bench("hamming/decode_64_bytes", || {
        decode_hamming(black_box(&encoded), data.len()).expect("decode")
    });
}

fn bench_arq() {
    let config = ArqConfig::new(4, 3).expect("config");
    let payload = vec![0x42u8; 64];

    bench("arq/send_ack_cycle_4_frames", || {
        let mut sender = ArqSender::new(config).expect("sender");
        let mut receiver = ArqReceiver::new(config).expect("receiver");
        for _ in 0..4 {
            let sequence = sender.send(payload.clone()).expect("send");
            receiver
                .receive(sequence, payload.clone())
                .expect("receive");
            sender.acknowledge(sequence).expect("acknowledge");
        }
    });
}
