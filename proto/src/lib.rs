//! Wire protocol shared by host and clients.
//! Packet = [type:u8][len:u32 BE][payload]. All ints big-endian.
use std::io::{self, Read, Write};

pub const PORT: u16 = 47990;
pub const MAX_PACKET: u32 = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum Packet {
    /// client -> host: PIN for pairing
    Hello { pin: String },
    /// host -> client: auth result + screen size
    Welcome { ok: bool, width: u16, height: u16 },
    /// host -> client: one H.264 Annex-B access unit
    Video { ts_ms: u64, keyframe: bool, data: Vec<u8> },
    /// client -> host: absolute pointer, normalized 0..1
    MouseMove { x: f32, y: f32 },
    /// button: 0 left, 1 right, 2 middle
    MouseButton { button: u8, down: bool },
    Scroll { dy: i16 },
    /// Windows virtual-key code
    Key { vk: u16, down: bool },
    /// client -> host: ask for a keyframe (after loss / start)
    RequestKeyframe,
}

fn put(w: &mut impl Write, t: u8, p: &[u8]) -> io::Result<()> {
    w.write_all(&[t])?;
    w.write_all(&(p.len() as u32).to_be_bytes())?;
    w.write_all(p)
}

impl Packet {
    pub fn write(&self, w: &mut impl Write) -> io::Result<()> {
        match self {
            Packet::Hello { pin } => put(w, 1, pin.as_bytes()),
            Packet::Welcome { ok, width, height } => {
                let mut b = vec![*ok as u8];
                b.extend(width.to_be_bytes());
                b.extend(height.to_be_bytes());
                put(w, 2, &b)
            }
            Packet::Video { ts_ms, keyframe, data } => {
                let mut b = Vec::with_capacity(9 + data.len());
                b.extend(ts_ms.to_be_bytes());
                b.push(*keyframe as u8);
                b.extend(data);
                put(w, 3, &b)
            }
            Packet::MouseMove { x, y } => {
                let mut b = x.to_be_bytes().to_vec();
                b.extend(y.to_be_bytes());
                put(w, 4, &b)
            }
            Packet::MouseButton { button, down } => put(w, 5, &[*button, *down as u8]),
            Packet::Scroll { dy } => put(w, 6, &dy.to_be_bytes()),
            Packet::Key { vk, down } => {
                let mut b = vk.to_be_bytes().to_vec();
                b.push(*down as u8);
                put(w, 7, &b)
            }
            Packet::RequestKeyframe => put(w, 8, &[]),
        }
    }

    pub fn read(r: &mut impl Read) -> io::Result<Packet> {
        let mut h = [0u8; 5];
        r.read_exact(&mut h)?;
        let len = u32::from_be_bytes([h[1], h[2], h[3], h[4]]);
        if len > MAX_PACKET {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "packet too large"));
        }
        let mut p = vec![0u8; len as usize];
        r.read_exact(&mut p)?;
        let bad = || io::Error::new(io::ErrorKind::InvalidData, "bad packet");
        Ok(match h[0] {
            1 => Packet::Hello { pin: String::from_utf8(p).map_err(|_| bad())? },
            2 if p.len() == 5 => Packet::Welcome {
                ok: p[0] != 0,
                width: u16::from_be_bytes([p[1], p[2]]),
                height: u16::from_be_bytes([p[3], p[4]]),
            },
            3 if p.len() >= 9 => Packet::Video {
                ts_ms: u64::from_be_bytes(p[0..8].try_into().unwrap()),
                keyframe: p[8] != 0,
                data: p[9..].to_vec(),
            },
            4 if p.len() == 8 => Packet::MouseMove {
                x: f32::from_be_bytes(p[0..4].try_into().unwrap()),
                y: f32::from_be_bytes(p[4..8].try_into().unwrap()),
            },
            5 if p.len() == 2 => Packet::MouseButton { button: p[0], down: p[1] != 0 },
            6 if p.len() == 2 => Packet::Scroll { dy: i16::from_be_bytes([p[0], p[1]]) },
            7 if p.len() == 3 => Packet::Key { vk: u16::from_be_bytes([p[0], p[1]]), down: p[2] != 0 },
            8 => Packet::RequestKeyframe,
            _ => return Err(bad()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let pk = Packet::Video { ts_ms: 42, keyframe: true, data: vec![1, 2, 3] };
        let mut buf = vec![];
        pk.write(&mut buf).unwrap();
        assert_eq!(Packet::read(&mut &buf[..]).unwrap(), pk);
    }
}
