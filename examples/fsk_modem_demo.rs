use cachetalk_modem::{
    decode_frame, decode_manchester, demodulate, encode_frame, encode_manchester, modulate,
    FskConfig,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = FskConfig::default();
    let payload = b"hello cachetalk";
    let frame = encode_frame(3, payload, true)?;
    let symbols = encode_manchester(&frame);
    let samples = modulate(&symbols, config)?;
    let recovered_bits = demodulate(&samples, symbols.len(), config)?;
    let recovered_frame = decode_manchester(&recovered_bits)?;
    let decoded = decode_frame(&recovered_frame)?;

    println!("FSK config: {config:?}");
    println!("Payload: {} bytes", payload.len());
    println!("Frame: {} bytes", frame.len());
    println!("Manchester symbols: {}", symbols.len());
    println!("Samples: {}", samples.len());
    println!("Decoded sequence: {}", decoded.sequence);
    println!(
        "Decoded payload: {}",
        String::from_utf8_lossy(&decoded.payload)
    );
    println!("ECC enabled: {}", decoded.ecc);

    assert_eq!(decoded.sequence, 3);
    assert_eq!(decoded.payload.as_slice(), &payload[..]);
    assert!(decoded.ecc);

    Ok(())
}
