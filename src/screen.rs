/// Conservative protocol boundary. An unsupported command disables drawing for the session.
#[derive(Default)]
pub struct ProtocolGuard {
    state: u8,
    sequence: Vec<u8>,
    pub blocked: bool,
    pub clear_generation: u64,
    continuation: u8,
}
impl ProtocolGuard {
    pub fn ready(&self) -> bool {
        !self.blocked && self.state == 0 && self.continuation == 0
    }
    pub fn observe(&mut self, bytes: &[u8]) {
        for &b in bytes {
            match self.state {
                1 => match b {
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
                        let allowed = match b {
                            b'A' | b'B' | b'C' | b'D' | b'E' | b'F' | b'G' | b'H' | b'f' | b'd'
                            | b'J' | b'K' | b'X' | b'P' | b'@' | b'L' | b'M' | b'S' | b'T'
                            | b'r' => self
                                .sequence
                                .iter()
                                .all(|v| v.is_ascii_digit() || *v == b';' || *v == b),
                            b'm' => supported_sgr(&self.sequence),
                            b'h' | b'l' => [
                                b"?25h".as_slice(),
                                b"?25l",
                                b"?1049h",
                                b"?1049l",
                                b"?2004h",
                                b"?2004l",
                                b"?1h",
                                b"?1l",
                            ]
                            .contains(&self.sequence.as_slice()),
                            _ => false,
                        };
                        if self.sequence == b"2J" {
                            self.clear_generation = self.clear_generation.wrapping_add(1);
                        }
                        if !allowed {
                            self.blocked = true;
                        }
                        self.state = 0;
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
