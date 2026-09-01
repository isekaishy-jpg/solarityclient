//! Owned WDBC tables corresponding to stock `WowClientDB` storage.

use std::io::Cursor;

use wow_cdbc::DbcHeader;

use crate::archive::{ArchiveDescriptor, AssetError, AssetPath};
use crate::file_stack::AssetStore;

/// Stable header metadata for a WDBC client database.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WdbcHeader {
    record_count: u32,
    field_count: u32,
    record_size: u32,
    string_block_size: u32,
}

impl WdbcHeader {
    /// Returns the number of fixed-size records.
    #[must_use]
    pub const fn record_count(self) -> u32 {
        self.record_count
    }

    /// Returns the number of fields described by each record.
    #[must_use]
    pub const fn field_count(self) -> u32 {
        self.field_count
    }

    /// Returns the size of one record in bytes.
    #[must_use]
    pub const fn record_size(self) -> u32 {
        self.record_size
    }

    /// Returns the declared string-block size in bytes.
    #[must_use]
    pub const fn string_block_size(self) -> u32 {
        self.string_block_size
    }
}

/// A validated stock WDBC file loaded through normal archive precedence.
pub struct WdbcTable {
    path: AssetPath,
    source: ArchiveDescriptor,
    header: WdbcHeader,
    bytes: Vec<u8>,
    record_data_offset: usize,
    string_block_offset: usize,
}

impl WdbcTable {
    /// Resolves and validates a WDBC table through the mounted asset stack.
    ///
    /// This deliberately consumes the arbitrary-file API. Database loading has
    /// no alternate archive search or loose-file path of its own.
    ///
    /// # Errors
    ///
    /// Returns normal asset resolution failures or
    /// [`AssetError::DatabaseDecode`] for a malformed/non-WDBC entry.
    pub fn load(store: &mut AssetStore, path: &AssetPath) -> Result<Self, AssetError> {
        let read = store.read(path)?;
        let mut cursor = Cursor::new(read.bytes());
        let dependency_header =
            DbcHeader::parse(&mut cursor).map_err(|source| AssetError::DatabaseDecode {
                path: path.clone(),
                message: source.to_string(),
            })?;

        let record_data_offset = DbcHeader::SIZE;
        let record_bytes = usize::try_from(dependency_header.record_count)
            .ok()
            .and_then(|count| {
                usize::try_from(dependency_header.record_size)
                    .ok()
                    .and_then(|size| count.checked_mul(size))
            })
            .ok_or_else(|| AssetError::DatabaseDecode {
                path: path.clone(),
                message: "record region size overflow".to_owned(),
            })?;
        let string_block_offset =
            record_data_offset
                .checked_add(record_bytes)
                .ok_or_else(|| AssetError::DatabaseDecode {
                    path: path.clone(),
                    message: "string block offset overflow".to_owned(),
                })?;
        let required_size = usize::try_from(dependency_header.string_block_size)
            .ok()
            .and_then(|size| string_block_offset.checked_add(size))
            .ok_or_else(|| AssetError::DatabaseDecode {
                path: path.clone(),
                message: "table size overflow".to_owned(),
            })?;
        if read.bytes().len() < required_size {
            return Err(AssetError::DatabaseDecode {
                path: path.clone(),
                message: format!(
                    "declared table size {required_size} exceeds entry size {}",
                    read.bytes().len()
                ),
            });
        }

        let header = WdbcHeader {
            record_count: dependency_header.record_count,
            field_count: dependency_header.field_count,
            record_size: dependency_header.record_size,
            string_block_size: dependency_header.string_block_size,
        };
        let source = read.source().clone();
        let bytes = read.into_bytes();

        Ok(Self {
            path: path.clone(),
            source,
            header,
            bytes,
            record_data_offset,
            string_block_offset,
        })
    }

    /// Returns the normalized archive path used to load this table.
    #[must_use]
    pub fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the archive selected by stock precedence.
    #[must_use]
    pub fn source(&self) -> &ArchiveDescriptor {
        &self.source
    }

    /// Returns validated fixed-layout metadata.
    #[must_use]
    pub const fn header(&self) -> WdbcHeader {
        self.header
    }

    /// Returns a raw fixed-size record, or `None` when the index is out of range.
    #[must_use]
    pub fn record(&self, index: u32) -> Option<&[u8]> {
        if index >= self.header.record_count {
            return None;
        }

        let record_size = usize::try_from(self.header.record_size).ok()?;
        let record_offset = usize::try_from(index)
            .ok()?
            .checked_mul(record_size)?
            .checked_add(self.record_data_offset)?;
        self.bytes
            .get(record_offset..record_offset.checked_add(record_size)?)
    }

    /// Returns the complete raw WDBC string block.
    #[must_use]
    pub fn string_block(&self) -> &[u8] {
        let size = self.header.string_block_size as usize;
        &self.bytes[self.string_block_offset..self.string_block_offset + size]
    }

    /// Resolves a NUL-terminated byte string by its stock string-block offset.
    ///
    /// The result remains bytes because build-12340 localized tables do not
    /// promise UTF-8 encoding at this boundary.
    #[must_use]
    pub fn string_bytes(&self, offset: u32) -> Option<&[u8]> {
        let bytes = self.string_block().get(usize::try_from(offset).ok()?..)?;
        let length = bytes.iter().position(|byte| *byte == 0)?;
        bytes.get(..length)
    }

    /// Reads one four-byte field from a fixed WDBC record.
    ///
    /// Typed table decoders use this checked boundary so packed or mismatched
    /// schemas cannot index beyond the layout validated by the table header.
    pub(super) fn field_u32(&self, record: u32, field: u32) -> Option<u32> {
        let offset = usize::try_from(field).ok()?.checked_mul(4)?;
        let bytes = self.record(record)?.get(offset..offset.checked_add(4)?)?;
        Some(u32::from_le_bytes(bytes.try_into().ok()?))
    }

    /// Reads one byte from a packed WDBC record.
    ///
    /// A small number of stock tables, notably `CharBaseInfo.dbc`, describe
    /// logical fields in the header but store them as bytes instead of the
    /// ordinary four-byte WDBC cells. Typed decoders must opt into this packed
    /// boundary explicitly after validating the exact record shape.
    pub(super) fn field_u8(&self, record: u32, field: u32) -> Option<u8> {
        self.record(record)?
            .get(usize::try_from(field).ok()?)
            .copied()
    }
}
