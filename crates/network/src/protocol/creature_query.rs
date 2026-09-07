//! Creature cache replies decoded by build 12340's 67B840 and 98D4C0.

use thiserror::Error;

/// Server-owned unit definition, shared by all instances of its entry.
#[derive(Clone, Debug, PartialEq)]
pub struct CreatureTemplate {
    entry: u32,
    strings: [Vec<u8>; 6],
    flags: u32,
    creature_type: u32,
    family: u32,
    rank: u32,
    kill_credits: [u32; 2],
    display_ids: [u32; 4],
    health_multiplier: f32,
    mana_multiplier: f32,
    racial_leader: bool,
    quest_items: [u32; 6],
    movement_id: u32,
}

impl CreatureTemplate {
    /// Returns the cache key from OBJECT_FIELD_ENTRY.
    #[must_use]
    pub const fn entry(&self) -> u32 {
        self.entry
    }

    /// Returns four names, sub-name, and description as native byte strings.
    #[must_use]
    pub const fn strings(&self) -> &[Vec<u8>; 6] {
        &self.strings
    }

    /// Returns the complete native type flags, including liquid suppression bit 22.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// Returns the server's creature category word without narrowing it.
    #[must_use]
    pub const fn creature_type(&self) -> u32 {
        self.creature_type
    }

    /// Returns the full family identifier read by the native cache.
    #[must_use]
    pub const fn family(&self) -> u32 {
        self.family
    }

    /// Returns the server's creature rank word.
    #[must_use]
    pub const fn rank(&self) -> u32 {
        self.rank
    }

    /// Returns both alternate kill-credit entries.
    #[must_use]
    pub const fn kill_credits(&self) -> &[u32; 2] {
        &self.kill_credits
    }

    /// Returns the four display identifiers in native order.
    #[must_use]
    pub const fn display_ids(&self) -> &[u32; 4] {
        &self.display_ids
    }

    /// Returns the authored health multiplier.
    #[must_use]
    pub const fn health_multiplier(&self) -> f32 {
        self.health_multiplier
    }

    /// Returns the authored mana multiplier.
    #[must_use]
    pub const fn mana_multiplier(&self) -> f32 {
        self.mana_multiplier
    }

    /// Reports the native nonzero-byte racial-leader flag.
    #[must_use]
    pub const fn racial_leader(&self) -> bool {
        self.racial_leader
    }

    /// Returns all six quest-item identifiers.
    #[must_use]
    pub const fn quest_items(&self) -> &[u32; 6] {
        &self.quest_items
    }

    /// Returns the final movement identifier.
    #[must_use]
    pub const fn movement_id(&self) -> u32 {
        self.movement_id
    }
}

/// Completion of one creature query, including the native missing-entry marker.
#[derive(Clone, Debug, PartialEq)]
pub enum CreatureQueryResponse {
    /// A complete server-owned definition.
    Found(Box<CreatureTemplate>),
    /// The entry was absent; the marker bit has been removed.
    Missing(u32),
}

impl CreatureQueryResponse {
    /// Returns the requested entry for either response form.
    #[must_use]
    pub fn entry(&self) -> u32 {
        match self {
            Self::Found(template) => template.entry(),
            Self::Missing(entry) => *entry,
        }
    }

    pub(crate) fn decode(payload: &[u8]) -> Result<Self, CreatureQueryPacketError> {
        let mut reader = TemplateReader { remaining: payload };
        let entry = reader.word()?;
        let response = if entry & 0x8000_0000 != 0 {
            Self::Missing(entry & 0x7fff_ffff)
        } else {
            let mut strings = std::array::from_fn(|_| Vec::new());
            for string in &mut strings {
                *string = reader.string()?;
            }
            Self::Found(Box::new(CreatureTemplate {
                entry,
                strings,
                flags: reader.word()?,
                creature_type: reader.word()?,
                family: reader.word()?,
                rank: reader.word()?,
                kill_credits: reader.words()?,
                display_ids: reader.words()?,
                health_multiplier: f32::from_bits(reader.word()?),
                mana_multiplier: f32::from_bits(reader.word()?),
                racial_leader: reader.byte()? != 0,
                quest_items: reader.words()?,
                movement_id: reader.word()?,
            }))
        };
        if !reader.remaining.is_empty() {
            return Err(CreatureQueryPacketError::TrailingBytes);
        }
        Ok(response)
    }
}

/// A reply that cannot form the complete native creature template.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum CreatureQueryPacketError {
    /// A required scalar or string terminator was absent.
    #[error("truncated creature query response")]
    Truncated,
    /// 98D4C0 supplies a 1024-byte buffer to the native string reader.
    #[error("creature query string exceeds 1023 bytes")]
    StringTooLong,
    /// The body extends beyond the selected response form.
    #[error("trailing bytes in creature query response")]
    TrailingBytes,
}

struct TemplateReader<'a> {
    remaining: &'a [u8],
}

impl TemplateReader<'_> {
    fn word(&mut self) -> Result<u32, CreatureQueryPacketError> {
        let Some((word, remaining)) = self.remaining.split_first_chunk::<4>() else {
            return Err(CreatureQueryPacketError::Truncated);
        };
        self.remaining = remaining;
        Ok(u32::from_le_bytes(*word))
    }

    fn words<const N: usize>(&mut self) -> Result<[u32; N], CreatureQueryPacketError> {
        let mut words = [0; N];
        for word in &mut words {
            *word = self.word()?;
        }
        Ok(words)
    }

    fn byte(&mut self) -> Result<u8, CreatureQueryPacketError> {
        let Some((&byte, remaining)) = self.remaining.split_first() else {
            return Err(CreatureQueryPacketError::Truncated);
        };
        self.remaining = remaining;
        Ok(byte)
    }

    fn string(&mut self) -> Result<Vec<u8>, CreatureQueryPacketError> {
        let Some(length) = self.remaining.iter().take(1024).position(|byte| *byte == 0) else {
            return Err(if self.remaining.len() >= 1024 {
                CreatureQueryPacketError::StringTooLong
            } else {
                CreatureQueryPacketError::Truncated
            });
        };
        let string = self.remaining[..length].to_vec();
        self.remaining = &self.remaining[length + 1..];
        Ok(string)
    }
}
