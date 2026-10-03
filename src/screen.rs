/// Conservative protocol boundary. An unsupported command disables drawing for the session.
#[derive(Default)]
pub struct ProtocolGuard {
    state: u8,
    sequence: Vec<u8>,
    pub blocked: bool,
    pub clear_generation: u64,
    continuation: u8,
    synchronized: bool,
}
impl ProtocolGuard {
    pub fn ready(&self) -> bool {
        !self.blocked && !self.synchronized && self.state == 0 && self.continuation == 0
    }
    pub fn observe(&mut self, bytes: &[u8]) {
        for &b in bytes {
            match self.state {
                1 => match b {
                    b']' => {
                        self.state = 3;
                        self.sequence.clear();
                    }
                    b'[' => {
                        self.state = 2;
                        self.sequence.clear();
                    }
                    b'7' | b'8' | b'D' | b'E' | b'M' | b'=' | b'>' => self.state = 0,
                    _ => {
                        self.blocked = true;
                        self.state = 0;
                    }
                },
                2 => {
                    if self.sequence.len() >= 128 {
                        self.blocked = true;
                    } else {
                        self.sequence.push(b);
                    }
                    if (0x40..=0x7e).contains(&b) {
                        let allowed = supported_csi(&self.sequence, b);
                        if self.sequence == b"?2026h" {
                            self.synchronized = true;
                        }
                        if self.sequence == b"?2026l" {
                            self.synchronized = false;
                        }
                        if self.sequence == b"2J" {
                            self.clear_generation = self.clear_generation.wrapping_add(1);
                        }
                        if !allowed {
                            self.blocked = true;
                        }
                        self.state = 0;
                    }
                }
                3 | 4 => {
                    if b == 7 || (self.state == 4 && b == b'\\') {
                        if !supported_osc(&self.sequence) {
                            self.blocked = true;
                        }
                        self.sequence.clear();
                        self.state = 0;
                    } else if b == 0x1b && self.state == 3 {
                        self.state = 4;
                    } else if self.state == 4 || self.sequence.len() >= 1024 {
                        self.blocked = true;
                    } else {
                        self.sequence.push(b);
                    }
                }
                _ => {
                    if b == 0x1b {
                        self.state = 1;
                        self.continuation = 0;
                    } else if self.continuation > 0 && b & 0xc0 == 0x80 {
                        self.continuation -= 1;
                    } else {
                        self.continuation = match b {
                            0xc2..=0xdf => 1,
                            0xe0..=0xef => 2,
                            0xf0..=0xf4 => 3,
                            _ => 0,
                        };
                        if (0x80..=0xbf).contains(&b) {
                            self.blocked = true;
                        }
                    }
                }
            }
        }
    }
}

fn supported_sgr(sequence: &[u8]) -> bool {
    let Some(body) = sequence.strip_suffix(b"m") else {
        return false;
    };
    let Ok(text) = std::str::from_utf8(body) else {
        return false;
    };
    let mut values = text
        .split(';')
        .map(|v| if v.is_empty() { Ok(0_u16) } else { v.parse() });
    while let Some(value) = values.next() {
        match value {
            Ok(0..=4 | 7 | 22..=24 | 27 | 30..=37 | 39..=47 | 49 | 90..=97 | 100..=107) => {}
            Ok(38 | 48) => {
                let count = match values.next() {
                    Some(Ok(2)) => 3,
                    Some(Ok(5)) => 1,
                    _ => return false,
                };
                for _ in 0..count {
                    if !matches!(values.next(), Some(Ok(0..=255))) {
                        return false;
                    }
                }
            }
            _ => return false,
        }
    }
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocols_fail_closed_and_sgr_is_exact() {
        for bytes in [
            b"\x1b[58;2;1;2;3m".as_slice(),
            b"\x1b[?6h",
            b"\x1b_Gimage",
            b"\x1b]8;;https://example.org\x07",
        ] {
            let mut g = ProtocolGuard::default();
            g.observe(bytes);
            assert!(!g.ready());
        }
        let mut g = ProtocolGuard::default();
        g.observe(b"\x1b[38;2;255;10;30m");
        assert!(g.ready());
        g.observe(b"\x1b[");
        assert!(!g.ready());
        g.observe(b"0m");
        assert!(g.ready());
    }
}

fn supported_osc(sequence: &[u8]) -> bool {
    sequence.starts_with(b"0;")
        || sequence.starts_with(b"1;")
        || sequence.starts_with(b"2;")
        || sequence == b"10;?"
        || sequence == b"11;?"
}

#[cfg(test)]
#[path = "../tests/hosts/hermes/protocol.rs"]
mod hermes_tests;

fn supported_csi(sequence: &[u8], final_byte: u8) -> bool {
    match final_byte {
        b'A' | b'B' | b'C' | b'D' | b'E' | b'F' | b'G' | b'H' | b'f' | b'd' | b'J' | b'K'
        | b'X' | b'P' | b'@' | b'L' | b'M' | b'S' | b'T' | b'r' => sequence
            .iter()
            .all(|v| v.is_ascii_digit() || *v == b';' || *v == final_byte),
        b'm' => supported_sgr(sequence) || sequence == b">4m",
        b'h' | b'l' => [
            b"?25h".as_slice(),
            b"?9l",
            b"?1001l",
            b"?1005l",
            b"?1015l",
            b"?1016l",
            b"?25l",
            b"?1049h",
            b"?1049l",
            b"?2004h",
            b"?2004l",
            b"?1h",
            b"?1l",
            b"?1000h",
            b"?1000l",
            b"?1002h",
            b"?1002l",
            b"?1003h",
            b"?1003l",
            b"?1004h",
            b"?1004l",
            b"?1006h",
            b"?1006l",
            b"?2029l",
            b"?2026h",
            b"?2026l",
        ]
        .contains(&sequence),
        b'z' => sequence == b"0'z",
        b'{' => sequence == b"0'{",
        b'p' => sequence == b"?1000$p",
        b'c' => sequence == b"c",
        b'q' => sequence == b">0q",
        b'u' => sequence == b"<u",
        _ => false,
    }
}
