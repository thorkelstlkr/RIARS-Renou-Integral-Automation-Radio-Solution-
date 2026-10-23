//! Small encoders for the Icecast HTTP requests, kept here to avoid extra dependencies.

/// Standard base64 with padding, for HTTP Basic auth.
pub fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Percent-encode everything except unreserved characters (RFC 3986).
pub fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64() {
        for (input, want) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foobar", "Zm9vYmFy"),
            ("source:hackme", "c291cmNlOmhhY2ttZQ=="),
        ] {
            assert_eq!(base64_encode(input.as_bytes()), want, "{input}");
        }
    }

    #[test]
    fn url() {
        assert_eq!(url_encode("abc"), "abc");
        assert_eq!(url_encode("a b"), "a%20b");
        assert_eq!(url_encode("Ni Permís"), "Ni%20Perm%C3%ADs");
        assert_eq!(url_encode("a&b=c/d"), "a%26b%3Dc%2Fd");
    }
}
