pub mod arq;
pub mod bits;
pub mod crc;
pub mod ecc;
pub mod frame;
pub mod fsk;
pub mod manchester;
pub mod tdm;

pub use arq::{ArqConfig, ArqError, ArqReceiver, ArqSender, InFlight};
pub use bits::{bits_to_bytes, bits_to_bytes_padded, bytes_to_bits, BitError};
pub use crc::{crc8, CRC8_INITIAL, CRC8_POLYNOMIAL};
pub use ecc::{decode_hamming, encode_hamming, encoded_len, EccError};
pub use frame::{
    decode_frame, encode_frame, Frame, FrameDecoder, FrameError, FLAG_ECC, FRAME_VERSION,
    HEADER_LEN, PREAMBLE, PREAMBLE_LEN,
};
pub use fsk::{demodulate, goertzel_power, modulate, FskConfig, FskError};
pub use manchester::{decode_manchester, encode_manchester, ManchesterDecoder, ManchesterError};
pub use tdm::{TdmConfig, TdmError};
