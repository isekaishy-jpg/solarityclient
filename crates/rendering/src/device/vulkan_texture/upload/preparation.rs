//! CPU-only authored mip expansion and packing on the existing flexible service workers.
use super::*;

/// Owned CPU staging payload, retained until its single ordered GPU transfer.
pub(super) struct PreparedTextureBatch {
    pub(super) bytes: Vec<u8>,
    pub(super) textures: Vec<PreparedImage>,
    pub(super) storage: Option<solarity_cpu::ByteReservation>,
}
type PreparedImage = (
    vk::Format,
    u32,
    (u32, u32),
    Vec<UploadMip>,
    BlpTextureResourceInfo,
);

fn prepare_textures(
    requests: &[(&BlpTextureSource, BlpColorSpace)],
    budget: Option<&solarity_cpu::CpuStorageBudget>,
) -> Result<PreparedTextureBatch, BlpTextureUploadError> {
    let mut staging_byte_count = 0_usize;
    for (source, _color_space) in requests {
        staging_byte_count = align_up(
            staging_byte_count,
            texel_block_byte_count(source_storage(source)),
        )
        .ok_or_else(|| AssetError::TextureDecode {
            path: source.path().clone(),
            message: "BLP batch staging alignment overflows".to_owned(),
        })?;
        staging_byte_count = staging_byte_count
            .checked_add(prepared_byte_count(source)?)
            .ok_or_else(|| AssetError::TextureDecode {
                path: source.path().clone(),
                message: "BLP batch staging byte count overflows".to_owned(),
            })?;
    }

    let storage = budget
        .map(|budget| {
            let bytes = staging_byte_count
                .checked_mul(3)
                .ok_or(solarity_cpu::CpuError::StorageAllocation)?;
            budget.reserve(
                solarity_cpu::CpuStorageClass::Required,
                solarity_cpu::CpuStorageKind::Result,
                bytes,
            )
        })
        .transpose()?;
    let mut staging_bytes = Vec::with_capacity(staging_byte_count);
    let mut prepared_textures = Vec::with_capacity(requests.len());
    for (source, color_space) in requests {
        let mut prepared = prepare_mips(source)?;
        let aligned_offset = align_up(
            staging_bytes.len(),
            texel_block_byte_count(prepared.storage),
        )
        .ok_or_else(|| AssetError::TextureDecode {
            path: source.path().clone(),
            message: "BLP batch staging alignment overflows".to_owned(),
        })?;
        staging_bytes.resize(aligned_offset, 0);
        let base_offset =
            u64::try_from(staging_bytes.len()).map_err(|error| AssetError::TextureDecode {
                path: source.path().clone(),
                message: format!("BLP batch staging offset is not addressable: {error}"),
            })?;
        for mip in &mut prepared.mips {
            mip.offset =
                mip.offset
                    .checked_add(base_offset)
                    .ok_or_else(|| AssetError::TextureDecode {
                        path: source.path().clone(),
                        message: "BLP batch mip offset overflows".to_owned(),
                    })?;
        }
        staging_bytes.extend_from_slice(&prepared.bytes);
        let format = texture_format(prepared.storage, *color_space);
        let mip_levels = u32::try_from(prepared.mips.len())
            .map_err(|error| VulkanError::operation("convert sampled image mip count", error))?;
        let info = BlpTextureResourceInfo::new(
            source.path().clone(),
            BlpTextureSourceKind::Authored,
            *color_space,
            prepared.storage,
            (source.width(), source.height()),
            prepared.mips.len(),
            prepared.bytes.len(),
        );
        prepared_textures.push((
            format,
            mip_levels,
            (source.width(), source.height()),
            prepared.mips,
            info,
        ));
    }
    if staging_bytes.len() != staging_byte_count {
        let source = requests[0].0;
        return Err(AssetError::TextureDecode {
            path: source.path().clone(),
            message: format!(
                "BLP batch contains {} upload bytes; expected {staging_byte_count}",
                staging_bytes.len()
            ),
        }
        .into());
    }

    Ok(PreparedTextureBatch {
        bytes: staging_bytes,
        textures: prepared_textures,
        storage,
    })
}
#[derive(Default)]
struct PreparationJob {
    sources: Vec<(BlpTextureSource, BlpColorSpace)>,
    budget: Option<solarity_cpu::CpuStorageBudget>,
    result: Option<Result<PreparedTextureBatch, BlpTextureUploadError>>,
}
pub(in crate::device::vulkan_texture) struct TexturePreparation {
    jobs: Vec<PreparationJob>,
    batch: solarity_cpu::LoadBatch<PreparationJob>,
}
impl Default for TexturePreparation {
    fn default() -> Self {
        Self {
            jobs: vec![PreparationJob::default()],
            batch: solarity_cpu::LoadBatch::new(solarity_cpu::CpuService::Required, |job| {
                let requests = job
                    .sources
                    .iter()
                    .map(|(source, color)| (source, *color))
                    .collect::<Vec<_>>();
                job.result = Some(prepare_textures(&requests, job.budget.as_ref()));
                job.sources.clear();
                job.budget = None;
                solarity_cpu::JobOutcome::Succeeded
            }),
        }
    }
}
impl TexturePreparation {
    pub(super) fn prepare(
        &mut self,
        execution: Option<&mut dyn crate::WorldFrameExecution>,
        requests: &[(&BlpTextureSource, BlpColorSpace)],
    ) -> Result<PreparedTextureBatch, BlpTextureUploadError> {
        let Some(execution) = execution else {
            return prepare_textures(requests, None);
        };
        let job = &mut self.jobs[0];
        job.sources.extend(
            requests
                .iter()
                .map(|(source, color)| ((*source).clone(), *color)),
        );
        job.budget = Some(execution.executor().storage().clone());
        job.result = None;
        if let Err(error) = self
            .batch
            .start_after(execution.executor(), &mut self.jobs, &[])
        {
            self.jobs[0].sources.clear();
            self.jobs[0].budget = None;
            return Err(error.into());
        }
        let ready =
            execution.wait_for_recording(&crate::WorldRecordingCompletion::loading(&self.batch));
        let reclaimed = self.batch.reclaim(&mut self.jobs);
        // Reclaim even after native failure/panic before dropping generation pins or outputs.
        let job = &mut self.jobs[0];
        job.sources.clear();
        job.budget = None;
        let result = job.result.take();
        ready?;
        reclaimed?;
        result.ok_or_else(|| {
            VulkanError::operation("prepare BLP upload", "worker result is unavailable")
        })?
    }
}
