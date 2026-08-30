//! Stock add-on identity records carried by `CMSG_AUTH_SESSION`.

use thiserror::Error;

const MAX_ADDON_COUNT: usize = 4_096;
const MAX_ADDON_NAME_BYTES: usize = 255;

/// One add-on reported to the selected world server.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldAddon {
    name: String,
    enabled: bool,
    crc: u32,
    unknown: u32,
}

impl WorldAddon {
    /// Creates a validated stock add-on identity record.
    ///
    /// # Errors
    ///
    /// Returns [`AddonManifestError::InvalidName`] when the name is empty or
    /// exceeds 255 bytes, or contains the NUL byte reserved as the wire terminator.
    pub fn new(
        name: impl Into<String>,
        enabled: bool,
        crc: u32,
        unknown: u32,
    ) -> Result<Self, AddonManifestError> {
        let name = name.into();
        if name.is_empty() {
            return Err(AddonManifestError::InvalidName {
                name,
                reason: "add-on names cannot be empty",
            });
        }
        if name.as_bytes().contains(&0) {
            return Err(AddonManifestError::InvalidName {
                name,
                reason: "add-on names cannot contain a NUL byte",
            });
        }
        if name.len() > MAX_ADDON_NAME_BYTES {
            return Err(AddonManifestError::InvalidName {
                name,
                reason: "add-on names cannot exceed 255 bytes",
            });
        }
        Ok(Self {
            name,
            enabled,
            crc,
            unknown,
        })
    }

    /// Returns the add-on folder name sent on the wire.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns whether the add-on is enabled for this login.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the add-on file CRC.
    #[must_use]
    pub const fn crc(&self) -> u32 {
        self.crc
    }

    /// Returns the trailing build-12340 add-on field with unknown semantics.
    #[must_use]
    pub const fn unknown(&self) -> u32 {
        self.unknown
    }

    fn encoded_len(&self) -> usize {
        self.name.len() + 1 + 1 + 4 + 4
    }

    fn encode_into(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(self.name.as_bytes());
        output.push(0);
        output.push(u8::from(self.enabled));
        output.extend_from_slice(&self.crc.to_le_bytes());
        output.extend_from_slice(&self.unknown.to_le_bytes());
    }
}

/// Ordered enabled add-ons reported during world authentication.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WorldAddonManifest {
    addons: Vec<WorldAddon>,
}

impl WorldAddonManifest {
    /// Creates a manifest in the exact discovery order supplied by the UI layer.
    ///
    /// # Errors
    ///
    /// Returns [`AddonManifestError::TooManyAddons`] when the input exceeds the
    /// explicit network resource bound.
    pub fn new(addons: Vec<WorldAddon>) -> Result<Self, AddonManifestError> {
        if addons.len() > MAX_ADDON_COUNT {
            return Err(AddonManifestError::TooManyAddons {
                count: addons.len(),
                maximum: MAX_ADDON_COUNT,
            });
        }
        Ok(Self { addons })
    }

    /// Creates the explicit stock representation of no enabled add-ons.
    #[must_use]
    pub const fn empty() -> Self {
        Self { addons: Vec::new() }
    }

    /// Returns the ordered add-on records.
    #[must_use]
    pub fn addons(&self) -> &[WorldAddon] {
        &self.addons
    }

    pub(crate) fn encode(&self) -> Vec<u8> {
        let capacity = 8 + self
            .addons
            .iter()
            .map(WorldAddon::encoded_len)
            .sum::<usize>();
        let mut output = Vec::with_capacity(capacity);
        output.extend_from_slice(&(self.addons.len() as u32).to_le_bytes());
        for addon in &self.addons {
            addon.encode_into(&mut output);
        }
        // The build-12340 client appends this field after all add-on records;
        // AzerothCore names it `unk4` and currently does not interpret it.
        output.extend_from_slice(&0_u32.to_le_bytes());
        output
    }
}

/// Invalid input while constructing a stock add-on manifest.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AddonManifestError {
    /// An add-on name cannot be represented as a stock CString.
    #[error("invalid add-on name {name:?}: {reason}")]
    InvalidName {
        /// Rejected name.
        name: String,
        /// Stable validation reason.
        reason: &'static str,
    },
    /// The enabled catalog exceeds the explicit network resource bound.
    #[error("add-on manifest contains {count} records; maximum is {maximum}")]
    TooManyAddons {
        /// Rejected record count.
        count: usize,
        /// Maximum supported record count.
        maximum: usize,
    },
}
