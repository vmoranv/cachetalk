use cachetalk_modem::{
    crc8, decode_frame, decode_manchester, demodulate, encode_frame, encode_manchester, modulate,
    Frame, FrameError, FskConfig,
};

fn round_trip(sequence: u8, payload: &[u8], ecc: bool) -> Frame {
    let config = FskConfig::default();
    let frame = encode_frame(sequence, payload, ecc).expect("frame encoding");
    let symbols = encode_manchester(&frame);
    let samples = modulate(&symbols, config).expect("modulation");
    let recovered_symbols = demodulate(&samples, symbols.len(), config).expect("demodulation");
    let recovered_frame = decode_manchester(&recovered_symbols).expect("manchester decoding");
    decode_frame(&recovered_frame).expect("frame decoding")
}

#[test]
fn ecc_round_trip_through_fsk() {
    let payload = b"hello";
    let decoded = round_trip(3, payload, true);

    assert_eq!(decoded.sequence, 3);
    assert_eq!(decoded.payload.as_slice(), &payload[..]);
    assert!(decoded.ecc);

    let frame = encode_frame(3, payload, true).expect("frame encoding");
    assert_eq!(frame.len(), 24);
    let symbols = encode_manchester(&frame);
    assert_eq!(symbols.len(), 384);
    let samples = modulate(&symbols, FskConfig::default()).expect("modulation");
    assert_eq!(samples.len(), 38400);
}

#[test]
fn plain_round_trip_through_fsk() {
    let payload = b"hello";
    let decoded = round_trip(7, payload, false);

    assert_eq!(decoded.sequence, 7);
    assert_eq!(decoded.payload.as_slice(), &payload[..]);
    assert!(!decoded.ecc);

    let frame = encode_frame(7, payload, false).expect("frame encoding");
    assert_eq!(frame.len(), 19);
    let symbols = encode_manchester(&frame);
    assert_eq!(symbols.len(), 304);
    let samples = modulate(&symbols, FskConfig::default()).expect("modulation");
    assert_eq!(samples.len(), 30400);
}

#[test]
fn ecc_corrects_single_bit_corruption() {
    let payload = b"hello";
    let mut frame = encode_frame(3, payload, true).expect("frame encoding");
    frame[13] ^= 0x01;

    let decoded = decode_frame(&frame).expect("single-bit error should be corrected");
    assert_eq!(decoded.payload.as_slice(), &payload[..]);
    assert!(decoded.ecc);
}

#[test]
fn plain_crc_detects_corruption() {
    let mut frame = encode_frame(3, b"hello", false).expect("frame encoding");
    frame[13] ^= 0x01;

    match decode_frame(&frame) {
        Err(FrameError::CrcMismatch { expected, actual }) => assert_ne!(expected, actual),
        other => panic!("expected CrcMismatch, got {other:?}"),
    }
}

#[test]
fn truncated_frame_is_incomplete() {
    let frame = encode_frame(3, b"hello", true).expect("frame encoding");
    let truncated = &frame[..frame.len() - 1];

    assert_eq!(decode_frame(truncated), Err(FrameError::Incomplete));
}

#[test]
fn trailing_bytes_are_reported() {
    let mut frame = encode_frame(3, b"hello", false).expect("frame encoding");
    frame.push(0);

    assert_eq!(decode_frame(&frame), Err(FrameError::TrailingBytes(1)));
}

#[test]
fn crc8_standard_check_value() {
    assert_eq!(crc8(b"123456789"), 0xFB);
}
