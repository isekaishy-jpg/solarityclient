//! One charge follows the immutable parsed source through clones and cache merges.

use crate::AssetReadBudget;
use solarity_cpu::{ByteReservation, CpuError, CpuStorageClass, CpuStorageKind};
use std::sync::Mutex;

#[derive(Default)]
struct Admission {
    charge: Option<ByteReservation>,
    required: bool,
}

pub(super) struct SourceStorage(Mutex<Admission>);

impl SourceStorage {
    pub(super) fn admit(
        budget: Option<&AssetReadBudget>,
        bytes: Result<usize, CpuError>,
    ) -> Result<Self, CpuError> {
        let storage = Self(Mutex::new(Admission::default()));
        storage.admit_for(budget, bytes?)?;
        Ok(storage)
    }

    pub(super) fn admit_for(
        &self,
        budget: Option<&AssetReadBudget>,
        bytes: usize,
    ) -> Result<(), CpuError> {
        let Some(budget) = budget else {
            return Ok(());
        };
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(|_| unreachable!("source accounting cannot panic"));
        let required = budget.class() == CpuStorageClass::Required;
        let already_required = state.required;
        match &mut state.charge {
            None => {
                state.charge = Some(budget.storage().reserve(
                    budget.class(),
                    CpuStorageKind::Result,
                    bytes,
                )?);
                state.required = required;
            }
            Some(_) if !required || already_required => {}
            Some(charge) => {
                charge.transfer(
                    budget.storage(),
                    CpuStorageClass::Required,
                    CpuStorageKind::Result,
                )?;
                state.required = true;
            }
        }
        Ok(())
    }

    pub(super) fn resize(&self, bytes: usize) -> Result<(), CpuError> {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(|_| unreachable!("source accounting cannot panic"));
        if let Some(charge) = &mut state.charge {
            charge.resize(bytes)?;
        }
        Ok(())
    }
}

impl std::fmt::Debug for SourceStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceStorage").finish_non_exhaustive()
    }
}

/// The parser copies at most sixteen declared mip ranges, a palette/JPEG header,
/// and vector descriptors. Count overlapping ranges separately. RAW3 allocates
/// from dimensions before checking input length, so admit that capacity too.
/// This is admission only: the ordinary parser still owns format validation.
pub(super) fn parse_bound(bytes: &[u8], metadata: usize) -> Result<usize, CpuError> {
    let word = |offset: usize| -> Option<usize> {
        let value = u32::from_le_bytes(bytes.get(offset..offset + 4)?.try_into().ok()?);
        usize::try_from(value).ok()
    };
    let mut bound = metadata
        .checked_add(bytes.len())
        .and_then(|value| value.checked_add(1024 + 32 * size_of::<wow_blp::Raw1Image>()))
        .ok_or(CpuError::StorageSizeOverflow)?;
    let table = if bytes.starts_with(b"BLP1") {
        92
    } else if bytes.starts_with(b"BLP2") {
        84
    } else {
        return Ok(bound);
    };
    let raw3 = bytes.starts_with(b"BLP2\x01\0\0\0\x03");
    for mip in 0..16 {
        let Some(size) = word(table + mip * 4) else {
            break;
        };
        let allocated = if raw3 && (mip == 0 || size != 0) {
            let width = (word(12).unwrap_or(0) >> mip).max(1);
            let height = (word(16).unwrap_or(0) >> mip).max(1);
            size.max(
                width
                    .checked_mul(height)
                    .and_then(|value| value.checked_mul(4))
                    .ok_or(CpuError::StorageSizeOverflow)?,
            )
        } else {
            size
        };
        bound = bound
            .checked_add(allocated)
            .ok_or(CpuError::StorageSizeOverflow)?;
    }
    Ok(bound)
}

#[allow(clippy::ptr_arg)]
fn vector_bytes<T>(values: &Vec<T>) -> usize {
    values.capacity().saturating_mul(size_of::<T>())
}

/// Retained capacities after parsing, including unused vector slots.
pub(super) fn image_bytes(image: &wow_blp::BlpImage) -> usize {
    use wow_blp::BlpContent;
    match &image.content {
        BlpContent::Dxt1(value) | BlpContent::Dxt3(value) | BlpContent::Dxt5(value) => {
            vector_bytes(&value.cmap)
                + vector_bytes(&value.images)
                + value
                    .images
                    .iter()
                    .map(|mip| vector_bytes(&mip.content))
                    .sum::<usize>()
        }
        BlpContent::Raw1(value) => {
            vector_bytes(&value.cmap)
                + vector_bytes(&value.images)
                + value
                    .images
                    .iter()
                    .map(|mip| vector_bytes(&mip.indexed_rgb) + vector_bytes(&mip.indexed_alpha))
                    .sum::<usize>()
        }
        BlpContent::Raw3(value) => {
            vector_bytes(&value.cmap)
                + vector_bytes(&value.images)
                + value
                    .images
                    .iter()
                    .map(|mip| vector_bytes(&mip.pixels))
                    .sum::<usize>()
        }
        BlpContent::Jpeg(value) => {
            vector_bytes(&value.header)
                + vector_bytes(&value.images)
                + value.images.iter().map(vector_bytes).sum::<usize>()
        }
    }
}
