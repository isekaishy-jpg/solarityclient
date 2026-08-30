//! Server add-on policy decoded from `SMSG_ADDON_INFO`.

use thiserror::Error;

use super::WorldAddonManifest;

const PUBLIC_KEY_BYTES: usize = 256;
const MAX_URL_BYTES: usize = 1_024;
const BANNED_ADDON_BYTES: usize = 44;

/// Policy returned for one manifest entry in the same wire order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldAddonPolicyEntry {
    name: String,
    state: u8,
    uses_public_key: bool,
    crc_mismatch: bool,
    public_key: Option<Box<[u8; PUBLIC_KEY_BYTES]>>,
    unknown: Option<u32>,
    url: Option<String>,
}

impl WorldAddonPolicyEntry {
    /// Returns the add-on name from the corresponding client manifest entry.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the exact policy-state byte sent by the world server.
    #[must_use]
    pub const fn state(&self) -> u8 {
        self.state
    }

    /// Returns whether the response carries the public-key policy branch.
    #[must_use]
    pub const fn uses_public_key(&self) -> bool {
        self.uses_public_key
    }

    /// Returns the server's CRC-mismatch flag.
    #[must_use]
    pub const fn crc_mismatch(&self) -> bool {
        self.crc_mismatch
    }

    /// Returns the replacement 256-byte key when the server supplied one.
    #[must_use]
    pub fn public_key(&self) -> Option<&[u8; PUBLIC_KEY_BYTES]> {
        self.public_key.as_deref()
    }

    /// Returns the trailing value present in the public-key policy branch.
    #[must_use]
    pub const fn unknown(&self) -> Option<u32> {
        self.unknown
    }

    /// Returns the optional server-provided add-on URL.
    #[must_use]
    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }
}

/// One server banned-add-on signature record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BannedAddon {
    id: u32,
    name_md5: [u8; 16],
    version_md5: [u8; 16],
    timestamp: u32,
    flags: u32,
}

impl BannedAddon {
    /// Returns the server-assigned banned-add-on identifier.
    #[must_use]
    pub const fn id(self) -> u32 {
        self.id
    }

    /// Returns the MD5 digest of the normalized add-on name.
    #[must_use]
    pub const fn name_md5(self) -> [u8; 16] {
        self.name_md5
    }

    /// Returns the MD5 digest of the banned add-on version.
    #[must_use]
    pub const fn version_md5(self) -> [u8; 16] {
        self.version_md5
    }

    /// Returns the server policy timestamp.
    #[must_use]
    pub const fn timestamp(self) -> u32 {
        self.timestamp
    }

    /// Returns the exact server policy flag bits.
    #[must_use]
    pub const fn flags(self) -> u32 {
        self.flags
    }
}

/// Complete positional policy response for the manifest sent at authentication.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WorldAddonPolicy {
    entries: Vec<WorldAddonPolicyEntry>,
    banned_addons: Vec<BannedAddon>,
}

impl WorldAddonPolicy {
    pub(crate) fn decode(
        payload: &[u8],
        manifest: &WorldAddonManifest,
    ) -> Result<Self, AddonPolicyError> {
        let mut cursor = PolicyCursor::new(payload);
        let mut entries = Vec::with_capacity(manifest.addons().len());
        for addon in manifest.addons() {
            let state = cursor.read_u8("add-on policy state is truncated")?;
            let uses_public_key = cursor.read_u8("add-on public-key flag is truncated")? != 0;
            let mut crc_mismatch = false;
            let mut public_key = None;
            let mut unknown = None;
            if uses_public_key {
                crc_mismatch = cursor.read_u8("add-on CRC state is truncated")? != 0;
                if crc_mismatch {
                    let key: [u8; PUBLIC_KEY_BYTES] = cursor
                        .take(PUBLIC_KEY_BYTES, "add-on public key is truncated")?
                        .try_into()
                        .map_err(|_| cursor.error("add-on public key is truncated"))?;
                    public_key = Some(Box::new(key));
                }
                unknown = Some(cursor.read_u32("add-on policy value is truncated")?);
            }
            let url = if cursor.read_u8("add-on URL flag is truncated")? != 0 {
                Some(cursor.read_cstring(MAX_URL_BYTES, "add-on URL is invalid")?)
            } else {
                None
            };
            entries.push(WorldAddonPolicyEntry {
                name: addon.name().to_owned(),
                state,
                uses_public_key,
                crc_mismatch,
                public_key,
                unknown,
                url,
            });
        }

        let banned_count = cursor.read_u32("banned-add-on count is truncated")? as usize;
        if banned_count > cursor.remaining() / BANNED_ADDON_BYTES {
            return Err(cursor.error("banned-add-on count exceeds the packet"));
        }
        let mut banned_addons = Vec::with_capacity(banned_count);
        for _ in 0..banned_count {
            banned_addons.push(BannedAddon {
                id: cursor.read_u32("banned-add-on identifier is truncated")?,
                name_md5: cursor.read_array("banned-add-on name digest is truncated")?,
                version_md5: cursor.read_array("banned-add-on version digest is truncated")?,
                timestamp: cursor.read_u32("banned-add-on timestamp is truncated")?,
                flags: cursor.read_u32("banned-add-on flags are truncated")?,
            });
        }
        if cursor.remaining() != 0 {
            return Err(cursor.error("add-on policy has trailing bytes"));
        }
        Ok(Self {
            entries,
            banned_addons,
        })
    }

    /// Returns positional policy entries paired with their manifest names.
    #[must_use]
    pub fn entries(&self) -> &[WorldAddonPolicyEntry] {
        &self.entries
    }

    /// Returns all banned-add-on signatures supplied by the server.
    #[must_use]
    pub fn banned_addons(&self) -> &[BannedAddon] {
        &self.banned_addons
    }

    /// Finds the first policy entry for an exact add-on folder name.
    #[must_use]
    pub fn by_name(&self, name: &str) -> Option<&WorldAddonPolicyEntry> {
        self.entries.iter().find(|entry| entry.name == name)
    }
}

/// A malformed `SMSG_ADDON_INFO` body.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("malformed add-on policy at byte {offset}: {message}")]
pub struct AddonPolicyError {
    offset: usize,
    message: &'static str,
}

impl AddonPolicyError {
    /// Returns the byte offset at which decoding failed.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// Returns a stable description of the rejected field.
    #[must_use]
    pub const fn message(&self) -> &'static str {
        self.message
    }
}

struct PolicyCursor<'a> {
    payload: &'a [u8],
    offset: usize,
}

impl<'a> PolicyCursor<'a> {
    const fn new(payload: &'a [u8]) -> Self {
        Self { payload, offset: 0 }
    }

    const fn error(&self, message: &'static str) -> AddonPolicyError {
        AddonPolicyError {
            offset: self.offset,
            message,
        }
    }

    const fn remaining(&self) -> usize {
        self.payload.len() - self.offset
    }

    fn take(&mut self, count: usize, message: &'static str) -> Result<&'a [u8], AddonPolicyError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| self.error(message))?;
        let bytes = self
            .payload
            .get(self.offset..end)
            .ok_or_else(|| self.error(message))?;
        self.offset = end;
        Ok(bytes)
    }

    fn read_u8(&mut self, message: &'static str) -> Result<u8, AddonPolicyError> {
        Ok(self.take(1, message)?[0])
    }

    fn read_u32(&mut self, message: &'static str) -> Result<u32, AddonPolicyError> {
        let bytes = self.read_array(message)?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn read_array<const N: usize>(
        &mut self,
        message: &'static str,
    ) -> Result<[u8; N], AddonPolicyError> {
        self.take(N, message)?
            .try_into()
            .map_err(|_| self.error(message))
    }

    fn read_cstring(
        &mut self,
        maximum: usize,
        message: &'static str,
    ) -> Result<String, AddonPolicyError> {
        let remaining = &self.payload[self.offset..];
        let length = remaining
            .iter()
            .take(maximum + 1)
            .position(|byte| *byte == 0)
            .ok_or_else(|| self.error(message))?;
        if length > maximum {
            return Err(self.error(message));
        }
        let bytes = self.take(length, message)?;
        let value = std::str::from_utf8(bytes)
            .map_err(|_| self.error(message))?
            .to_owned();
        self.offset += 1;
        Ok(value)
    }
}
