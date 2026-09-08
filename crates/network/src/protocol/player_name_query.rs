//! Name-cache results read by the original 6357D0 handler.

use thiserror::Error;

/// Name and identity bytes retained by the build-12340 name cache.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldPlayerName {
    /// Name, bounded by the native 48-byte destination including terminator.
    pub name: String,
    /// Realm, bounded by the native 256-byte destination.
    pub realm: String,
    /// Server-authored race byte.
    pub race: u8,
    /// Server-authored gender byte.
    pub gender: u8,
    /// Server-authored class byte.
    pub class: u8,
    /// Optional five declined name forms, each bounded by 64 bytes.
    pub declined: Option<[String; 5]>,
    /// Native +188 flag for status three and 1FD GUIDs.
    pub flagged: bool,
}

/// The cache operation selected by the status byte.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorldPlayerNameResult {
    /// Publishes the record before completing pending callbacks.
    Found(WorldPlayerName),
    /// Status two requeues an existing lookup without completing callbacks.
    Retry,
    /// Other nonzero statuses complete callbacks and remove the cache entry.
    Missing,
}

/// SMSG_NAME_QUERY_RESPONSE (51), with a packed full GUID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldPlayerNameResponse {
    /// Requested source GUID.
    pub guid: u64,
    /// Decoded cache result.
    pub result: WorldPlayerNameResult,
}

/// A name result has a truncated field or an invalid bounded C string.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("malformed name query response")]
pub struct WorldPlayerNamePacketError;

impl WorldPlayerNameResponse {
    pub(super) fn decode(
        opcode: u16,
        body: &[u8],
    ) -> Result<Option<Self>, WorldPlayerNamePacketError> {
        if opcode != 0x51 {
            return Ok(None);
        }
        let mut c = Cursor { body, offset: 0 };
        let mask = c.byte()?;
        let mut bytes = [0; 8];
        for (i, b) in bytes.iter_mut().enumerate() {
            if mask & (1 << i) != 0 {
                *b = c.byte()?;
            }
        }
        let guid = u64::from_le_bytes(bytes);
        let result = match c.byte()? {
            0 => {
                let name = c.string(48)?;
                let realm = c.string(256)?;
                let race = c.byte()?;
                let gender = c.byte()?;
                let class = c.byte()?;
                let declined = if c.byte()? != 0 {
                    Some([
                        c.string(64)?,
                        c.string(64)?,
                        c.string(64)?,
                        c.string(64)?,
                        c.string(64)?,
                    ])
                } else {
                    None
                };
                WorldPlayerNameResult::Found(WorldPlayerName {
                    name,
                    realm,
                    race,
                    gender,
                    class,
                    declined,
                    flagged: (guid >> 32) & 0xfff0_0000 == 0x1fd0_0000,
                })
            }
            2 => WorldPlayerNameResult::Retry,
            3 => WorldPlayerNameResult::Found(WorldPlayerName {
                name: "?".into(),
                realm: String::new(),
                race: 0,
                gender: 0,
                class: 0,
                declined: None,
                flagged: true,
            }),
            _ => WorldPlayerNameResult::Missing,
        };
        Ok(Some(Self { guid, result }))
    }
}

struct Cursor<'a> {
    body: &'a [u8],
    offset: usize,
}
impl Cursor<'_> {
    fn byte(&mut self) -> Result<u8, WorldPlayerNamePacketError> {
        let b = *self
            .body
            .get(self.offset)
            .ok_or(WorldPlayerNamePacketError)?;
        self.offset += 1;
        Ok(b)
    }
    fn string(&mut self, capacity: usize) -> Result<String, WorldPlayerNamePacketError> {
        let tail = self
            .body
            .get(self.offset..)
            .ok_or(WorldPlayerNamePacketError)?;
        let end = tail
            .iter()
            .take(capacity)
            .position(|&b| b == 0)
            .ok_or(WorldPlayerNamePacketError)?;
        let value = std::str::from_utf8(&tail[..end])
            .map_err(|_| WorldPlayerNamePacketError)?
            .to_owned();
        self.offset += end + 1;
        Ok(value)
    }
}
