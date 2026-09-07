//! Game-object cache replies decoded by build 12340's `0x0067BEE0`/`0x0098D750`.

use thiserror::Error;

/// One server-owned template, shared by all instances of an entry.
#[derive(Clone, Debug, PartialEq)]
pub struct GameObjectTemplate {
    entry: u32,
    object_type: u32,
    display_id: u32,
    /// Four names, icon, cast-bar caption, and the final unnamed native string.
    /// Native strings are byte strings; decoding does not impose UTF-8 validity.
    strings: [Vec<u8>; 7],
    properties: [u32; 24],
    scale: f32,
    quest_items: [u32; 6],
}

impl GameObjectTemplate {
    /// Returns the cache key, independent of any instance GUID.
    #[must_use]
    pub const fn entry(&self) -> u32 {
        self.entry
    }

    /// Returns the native behavior type used to interpret the property words.
    #[must_use]
    pub const fn object_type(&self) -> u32 {
        self.object_type
    }

    /// Returns the GameObjectDisplayInfo.dbc identifier.
    #[must_use]
    pub const fn display_id(&self) -> u32 {
        self.display_id
    }

    /// Returns the four names followed by icon, cast-bar caption, and unknown text.
    #[must_use]
    pub const fn strings(&self) -> &[Vec<u8>; 7] {
        &self.strings
    }

    /// Returns all 24 words copied by `0x0098D750`, without type-dependent coercion.
    #[must_use]
    pub const fn properties(&self) -> &[u32; 24] {
        &self.properties
    }

    /// Returns the scale bits decoded as the native floating-point field.
    #[must_use]
    pub const fn scale(&self) -> f32 {
        self.scale
    }

    /// Returns the six quest-item identifiers following the scale.
    #[must_use]
    pub const fn quest_items(&self) -> &[u32; 6] {
        &self.quest_items
    }
}

/// A completed query, including the native high-bit missing-entry response.
#[derive(Clone, Debug, PartialEq)]
pub enum GameObjectQueryResponse {
    /// The server supplied a complete template.
    Found(Box<GameObjectTemplate>),
    /// The entry was not found; this value has the wire marker bit removed.
    Missing(u32),
}

impl GameObjectQueryResponse {
    /// Returns the query's template key for either completion outcome.
    #[must_use]
    pub fn entry(&self) -> u32 {
        match self {
            Self::Found(template) => template.entry(),
            Self::Missing(entry) => *entry,
        }
    }

    /// Decodes the exact stock body, retaining all properties rather than the
    /// six-word subset in the pinned third-party message schema.
    pub(crate) fn decode(payload: &[u8]) -> Result<Self, GameObjectQueryPacketError> {
        let mut reader = TemplateReader { remaining: payload };
        let entry = reader.word()?;
        let result = if entry & 0x8000_0000 != 0 {
            Self::Missing(entry & 0x7fff_ffff)
        } else {
            let object_type = reader.word()?;
            let display_id = reader.word()?;
            let mut strings = std::array::from_fn(|_| Vec::new());
            for string in &mut strings {
                *string = reader.string()?;
            }
            let mut properties = [0; 24];
            for word in &mut properties {
                *word = reader.word()?;
            }
            let scale = f32::from_bits(reader.word()?);
            let mut quest_items = [0; 6];
            for item in &mut quest_items {
                *item = reader.word()?;
            }
            Self::Found(Box::new(GameObjectTemplate {
                entry,
                object_type,
                display_id,
                strings,
                properties,
                scale,
                quest_items,
            }))
        };
        if !reader.remaining.is_empty() {
            return Err(GameObjectQueryPacketError::TrailingBytes);
        }
        Ok(result)
    }
}

/// A query body that cannot form the complete native template representation.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum GameObjectQueryPacketError {
    /// A required word or string terminator is absent.
    #[error("truncated game-object query response")]
    Truncated,
    /// Native `0x0047B480` requires the terminator within its 1024-byte buffer.
    #[error("game-object query string exceeds 1023 bytes")]
    StringTooLong,
    /// The body contains bytes outside the selected response form.
    #[error("trailing bytes in game-object query response")]
    TrailingBytes,
}

/// A bounded reader whose failures never advance the encrypted packet stream.
struct TemplateReader<'a> {
    remaining: &'a [u8],
}

impl TemplateReader<'_> {
    /// Consumes one little-endian native field.
    fn word(&mut self) -> Result<u32, GameObjectQueryPacketError> {
        let Some((word, remaining)) = self.remaining.split_first_chunk::<4>() else {
            return Err(GameObjectQueryPacketError::Truncated);
        };
        self.remaining = remaining;
        Ok(u32::from_le_bytes(*word))
    }

    /// Copies at most the bytes admitted by the stock cache string reader.
    fn string(&mut self) -> Result<Vec<u8>, GameObjectQueryPacketError> {
        let Some(length) = self.remaining.iter().take(1024).position(|byte| *byte == 0) else {
            return Err(if self.remaining.len() >= 1024 {
                GameObjectQueryPacketError::StringTooLong
            } else {
                GameObjectQueryPacketError::Truncated
            });
        };
        let result = self.remaining[..length].to_vec();
        self.remaining = &self.remaining[length + 1..];
        Ok(result)
    }
}
