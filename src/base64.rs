//! Standard base64 encoding (RFC 4648, with `=` padding).
//!
//! This is a private module: the only caller is [the OSC 52
//! write](crate::TerminalDriver::set_clipboard), and the crate takes no
//! dependency for the one place that needs an encoding. Only the encoder is
//! here, because tuinix only ever produces a payload.

use std::io::{self, Write};

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Writes `text`'s UTF-8 bytes to `w` as standard base64.
///
/// The bytes are written directly, without building an intermediate `String`,
/// so a large payload does not allocate a copy of itself.
///
/// # Errors
///
/// Returns an error only if writing to `w` fails.
pub fn encode<W: Write>(w: &mut W, text: &str) -> io::Result<()> {
    for chunk in text.as_bytes().chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;

        let mut quad = [b'='; 4];
        quad[0] = ALPHABET[(n >> 18) as usize & 0x3f];
        quad[1] = ALPHABET[(n >> 12) as usize & 0x3f];
        if chunk.len() > 1 {
            quad[2] = ALPHABET[(n >> 6) as usize & 0x3f];
        }
        if chunk.len() > 2 {
            quad[3] = ALPHABET[n as usize & 0x3f];
        }
        w.write_all(&quad)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::encode;

    fn encode_to_string(text: &str) -> String {
        let mut out = Vec::new();
        encode(&mut out, text).expect("ok");
        String::from_utf8(out).expect("base64 is ASCII")
    }

    #[test]
    fn matches_known_vectors() {
        // Expected outputs are written out literally so a bug in the encoder
        // cannot pass both sides of the comparison.
        assert_eq!(encode_to_string(""), "");
        // Length 3n: no padding.
        assert_eq!(encode_to_string("Man"), "TWFu");
        // Length 3n-1: one `=` of padding.
        assert_eq!(encode_to_string("Ma"), "TWE=");
        // Length 3n-2: two `=` of padding.
        assert_eq!(encode_to_string("M"), "TQ==");
        // Inputs that exercise the ends of the alphabet.
        assert_eq!(encode_to_string("\u{0}"), "AA==");
        assert_eq!(encode_to_string("///"), "Ly8v");
        assert_eq!(encode_to_string("hello world"), "aGVsbG8gd29ybGQ=");
        // Multi-byte UTF-8 is encoded as its bytes, not its code points.
        assert_eq!(encode_to_string("\u{3042}"), "44GC");
    }
}
