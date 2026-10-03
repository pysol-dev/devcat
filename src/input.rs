#[derive(Default)]
enum State {
    #[default]
    Ground,
    Escape,
    Csi,
    Ss3,
    String(bool),
    Mouse(u8),
}
#[derive(Default)]
pub struct InputClassifier {
    state: State,
    sequence: [u8; 32],
    length: usize,
    paste: bool,
    utf8: [u8; 4],
    utf8_len: usize,
    utf8_expected: usize,
}
impl InputClassifier {
    /// Observes bytes without retaining text. The caller forwards the original slice unchanged.
    pub fn observe(&mut self, bytes: &[u8], mut strike: impl FnMut()) {
        for &byte in bytes {
            match self.state {
                State::Mouse(remaining) => {
                    self.state = if remaining <= 1 {
                        State::Ground
                    } else {
                        State::Mouse(remaining - 1)
                    };
                    continue;
                }
                State::String(escaped) => {
                    self.state = if byte == 7 || (escaped && byte == b'\\') {
                        State::Ground
                    } else {
                        State::String(byte == 0x1b)
                    };
                    continue;
                }
                State::Escape => {
                    self.state = match byte {
                        b'[' => {
                            self.length = 0;
                            State::Csi
                        }
                        b'O' => State::Ss3,
                        b']' | b'P' | b'_' | b'^' => State::String(false),
                        _ => State::Ground,
                    };
                    continue;
                }
                State::Ss3 => {
                    self.state = State::Ground;
                    continue;
                }
                State::Csi => {
                    if self.length < self.sequence.len() {
                        self.sequence[self.length] = byte;
                        self.length += 1;
                    }
                    if (0x40..=0x7e).contains(&byte) {
                        let seq = &self.sequence[..self.length];
                        if seq == b"200~" {
                            if !self.paste {
                                strike();
                            }
                            self.paste = true;
                        }
                        if seq == b"201~" {
                            self.paste = false;
                        }
                        self.state = if seq == b"M" {
                            State::Mouse(3)
                        } else {
                            State::Ground
                        };
                        self.length = 0;
                    }
                    continue;
                }
                State::Ground => {}
            }
            if byte == 0x1b {
                self.state = State::Escape;
                self.utf8_len = 0;
                self.utf8_expected = 0;
                continue;
            }
            if self.paste {
                continue;
            }
            if self.utf8_expected > 0 {
                if byte & 0xc0 == 0x80 {
                    self.utf8[self.utf8_len] = byte;
                    self.utf8_len += 1;
                    if self.utf8_len == self.utf8_expected {
                        if std::str::from_utf8(&self.utf8[..self.utf8_len]).is_ok() {
                            strike();
                        }
                        self.utf8_len = 0;
                        self.utf8_expected = 0;
                    }
                    continue;
                }
                self.utf8_len = 0;
                self.utf8_expected = 0;
            }
            match byte {
                0x08 | 0x7f | 0x20..=0x7e => strike(),
                0xc2..=0xdf => self.start_utf8(byte, 2),
                0xe0..=0xef => self.start_utf8(byte, 3),
                0xf0..=0xf4 => self.start_utf8(byte, 4),
                _ => {}
            }
        }
    }
    fn start_utf8(&mut self, byte: u8, length: usize) {
        self.utf8[0] = byte;
        self.utf8_len = 1;
        self.utf8_expected = length;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incremental_unicode_paste_and_replies() {
        let mut c = InputClassifier::default();
        let mut n = 0;
        for b in "é🙂".as_bytes() {
            c.observe(&[*b], || n += 1);
        }
        assert_eq!(n, 2);
        for b in b"\x1b[A\x1b[<0;5;6M\x1b[I\x1b[12;4R\x1b[200~long paste\x1b[201~\x7f" {
            c.observe(&[*b], || n += 1);
        }
        assert_eq!(n, 4);
    }
}

#[cfg(test)]
mod protocol_tests {
    use super::*;
    #[test]
    fn string_replies_and_legacy_mouse_do_not_strike() {
        let mut c = InputClassifier::default();
        let mut n = 0;
        for b in b"\x1b]10;rgb:aaaa/bbbb/cccc\x1b\\\x1b[M !!a" {
            c.observe(&[*b], || n += 1);
        }
        assert_eq!(n, 1);
    }
}
