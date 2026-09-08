//! Build-12340 aura slots read by `7300A0` and `716510`.

use thiserror::Error;

/// One slot in an authoritative aura update.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldUnitAura {
    /// Native unsigned-byte slot index.
    pub slot: u8,
    /// Zero removes the slot without further wire fields.
    pub spell: u32,
    /// Enabled effect bits and conditional wire flags.
    pub flags: u8,
    /// Caster level, preserved as transmitted.
    pub level: u8,
    /// Application count, preserved as transmitted.
    pub applications: u8,
    /// Caster GUID; flag 8 substitutes the owning unit's GUID.
    pub caster: u64,
    /// Authored maximum and remaining milliseconds when flag 20 is present.
    pub duration: Option<(u32, u32)>,
}

/// An ordered replacement or delta for a unit's aura bank.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldUnitAuraUpdate {
    /// Owning unit's packed GUID.
    pub guid: u64,
    /// Opcode 495 clears all slots before applying its records.
    pub replace: bool,
    /// Records in packet order, including repeated indices.
    pub auras: Vec<WorldUnitAura>,
}

/// The packet ended inside an aura record.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("truncated unit aura packet")]
pub struct WorldUnitAuraPacketError;

impl WorldUnitAuraUpdate {
    pub(super) fn decode(
        opcode: u16,
        body: &[u8],
    ) -> Result<Option<Self>, WorldUnitAuraPacketError> {
        if !matches!(opcode, 0x495 | 0x496) {
            return Ok(None);
        }
        let mut cursor = Cursor { body, offset: 0 };
        let guid = cursor.packed()?;
        let mut auras = Vec::new();
        while cursor.offset != body.len() {
            let slot = cursor.byte()?;
            let spell = cursor.word()?;
            let mut aura = WorldUnitAura {
                slot,
                spell,
                flags: 0,
                level: 0,
                applications: 0,
                caster: 0,
                duration: None,
            };
            if spell != 0 {
                aura.flags = cursor.byte()?;
                aura.level = cursor.byte()?;
                aura.applications = cursor.byte()?;
                aura.caster = if aura.flags & 8 != 0 {
                    guid
                } else {
                    cursor.packed()?
                };
                if aura.flags & 0x20 != 0 {
                    aura.duration = Some((cursor.word()?, cursor.word()?));
                }
            }
            auras.push(aura);
        }
        Ok(Some(Self {
            guid,
            replace: opcode == 0x495,
            auras,
        }))
    }
}

struct Cursor<'a> {
    body: &'a [u8],
    offset: usize,
}
impl Cursor<'_> {
    fn byte(&mut self) -> Result<u8, WorldUnitAuraPacketError> {
        let value = self
            .body
            .get(self.offset)
            .copied()
            .ok_or(WorldUnitAuraPacketError)?;
        self.offset += 1;
        Ok(value)
    }
    fn word(&mut self) -> Result<u32, WorldUnitAuraPacketError> {
        let value = self
            .body
            .get(self.offset..self.offset + 4)
            .ok_or(WorldUnitAuraPacketError)?;
        self.offset += 4;
        Ok(u32::from_le_bytes(
            value.try_into().map_err(|_| WorldUnitAuraPacketError)?,
        ))
    }
    fn packed(&mut self) -> Result<u64, WorldUnitAuraPacketError> {
        let mask = self.byte()?;
        let mut value = [0; 8];
        for (index, byte) in value.iter_mut().enumerate() {
            if mask & (1 << index) != 0 {
                *byte = self.byte()?;
            }
        }
        Ok(u64::from_le_bytes(value))
    }
}
