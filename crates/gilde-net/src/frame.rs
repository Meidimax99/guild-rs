//! Wire framing (`srv_RecvRecord` `0x10009ac0`): a record is sent as its
//! first `length` bytes, where bytes 0..3 are the header (type, u16 length).
//! The receiver reads the 3 header bytes, then the rest.

use gilde_sim::command::{Command, RECORD_SIZE};

/// Encode a command as one frame.
pub fn encode(cmd: &Command) -> Vec<u8> {
    let r = cmd.to_record();
    let len = u16::from_le_bytes([r[1], r[2]]) as usize;
    r[..len.clamp(3, RECORD_SIZE)].to_vec()
}

/// Incremental decoder for a byte stream.
#[derive(Default)]
pub struct Decoder {
    buf: Vec<u8>,
}

impl Decoder {
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// The next complete command, if one has arrived. Malformed lengths
    /// (shorter than the header or longer than a record) yield an error.
    pub fn next_command(&mut self) -> Result<Option<Command>, String> {
        if self.buf.len() < 3 {
            return Ok(None);
        }
        let len = u16::from_le_bytes([self.buf[1], self.buf[2]]) as usize;
        if !(3..=RECORD_SIZE).contains(&len) {
            return Err(format!("invalid frame length {len}"));
        }
        if self.buf.len() < len {
            return Ok(None);
        }
        let mut rec = [0u8; RECORD_SIZE];
        rec[..len].copy_from_slice(&self.buf[..len]);
        self.buf.drain(..len);
        Ok(Command::from_record(&rec))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gilde_sim::Date;

    #[test]
    fn stream_round_trip() {
        let a = Command::set_date(&Date::start_of_round(2));
        let b = Command::system(7, 3);
        let mut bytes = encode(&a);
        bytes.extend(encode(&b));
        let mut d = Decoder::default();
        d.push(&bytes[..10]);
        assert_eq!(d.next_command().unwrap(), None);
        d.push(&bytes[10..]);
        assert_eq!(d.next_command().unwrap(), Some(a));
        assert_eq!(d.next_command().unwrap(), Some(b));
        assert_eq!(d.next_command().unwrap(), None);
    }
}
