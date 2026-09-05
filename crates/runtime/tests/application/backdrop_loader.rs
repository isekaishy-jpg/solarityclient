//! Regression tests for worker-owned backdrop residency and admission.

use std::error::Error;
use std::io::Cursor;
use std::num::NonZeroUsize;
use std::sync::{Arc, mpsc};

use solarity_asset::{ArchiveCatalog, AssetError, AssetPath, ClientDataRoot, Locale};
use solarity_cpu::{CpuExecutor, CpuPoolConfig};
use wow_m2::chunks::texture::{M2Texture as RawTexture, M2TextureFlags, M2TextureType};
use wow_m2::common::{FixedString, M2Array, M2ArrayString};
use wow_m2::header::M2Header;
use wow_m2::skin::OldSkinHeader;
use wow_m2::{M2Model, M2Version, OldSkin};

use super::{GlueBackdropLoader, GlueM2Texture, RuntimeGlueModelError};
use crate::test_support::{self as support, ClientFixture};

fn loader(fixture: &ClientFixture) -> Result<GlueBackdropLoader, Box<dyn Error>> {
    Ok(GlueBackdropLoader::new(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?))
}

fn cpu() -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
        NonZeroUsize::MIN,
    ))?)
}

#[test]
fn full_pool_defers_archive_mount_and_preserves_request() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::new()?;
    let mut loader = loader(&fixture)?;
    let cpu = cpu()?;
    let path = AssetPath::new("Interface/Glues/Missing.m2")?;
    let (release, wait) = mpsc::channel();
    let blocker = cpu.try_submit(move || wait.recv())?;
    let deferred = loader.poll(&path, &cpu);
    // Release before asserting so a failing regression cannot strand a worker.
    release.send(())?;
    blocker.join()??;
    assert!(matches!(deferred, Ok(None)));
    assert!(!loader.has_pending());
    assert!(
        loader
            .assets
            .lock()
            .map_err(|_| "poisoned owner")?
            .state
            .is_none()
    );
    assert!(matches!(loader.load_blocking(&path, &cpu), Err(source)
        if matches!(source.as_ref(), RuntimeGlueModelError::Asset(AssetError::AssetNotFound { path: missing }) if *missing == path)));
    Ok(())
}

#[test]
fn changed_selection_retains_original_exact_failure_without_retry() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::new()?;
    let mut loader = loader(&fixture)?;
    let cpu = cpu()?;
    let first = AssetPath::new("Interface/Glues/First.m2")?;
    let second = AssetPath::new("Interface/Glues/Second.m2")?;
    assert!(matches!(loader.poll(&first, &cpu), Ok(None)));
    assert!(loader.load_blocking(&second, &cpu).is_err());
    let original = match loader.load_blocking(&first, &cpu) {
        Err(error) => error,
        Ok(_) => return Err("missing first model unexpectedly loaded".into()),
    };
    assert!(
        matches!(original.as_ref(), RuntimeGlueModelError::Asset(AssetError::AssetNotFound { path }) if *path == first)
    );
    let repeated = match loader.poll(&first, &cpu) {
        Err(error) => error,
        Ok(_) => return Err("missing model failure was not retained".into()),
    };
    assert!(Arc::ptr_eq(&original, &repeated));
    assert!(!loader.has_pending());
    assert_eq!(loader.ready.len(), 2);
    Ok(())
}

#[test]
fn demand_reuses_worker_model_after_selection_changes() -> Result<(), Box<dyn Error>> {
    let mut model = M2Model {
        header: M2Header::new(M2Version::WotLK),
        name: Some("Backdrop".to_owned()),
        ..M2Model::default()
    };
    model.header.num_skin_profiles = Some(1);
    let texture_names: [&[u8]; 3] = [
        b"Interface\\Glues\\Backdrop.blp",
        b"Interface\\Glues\\Missing.blp",
        b"",
    ];
    model.textures = texture_names
        .iter()
        .map(|name| {
            Ok(RawTexture {
                texture_type: M2TextureType::Hardcoded,
                flags: M2TextureFlags::empty(),
                filename: M2ArrayString {
                    string: FixedString {
                        data: name.to_vec(),
                    },
                    array: M2Array::new(u32::try_from(name.len() + 1)?, 0),
                },
            })
        })
        .collect::<Result<Vec<_>, std::num::TryFromIntError>>()?;
    let mut model_bytes = Cursor::new(Vec::new());
    model.write(&mut model_bytes)?;
    // The fixture writer does not relocate string payloads in texture entries.
    let model_bytes = model_bytes.get_mut();
    let textures_offset = u32::from_le_bytes(model_bytes[0x54..0x58].try_into()?) as usize;
    for (index, name) in texture_names.iter().enumerate() {
        let offset = u32::try_from(model_bytes.len())?;
        let entry = textures_offset + index * 16;
        model_bytes[entry + 8..entry + 12]
            .copy_from_slice(&u32::try_from(name.len() + 1)?.to_le_bytes());
        model_bytes[entry + 12..entry + 16].copy_from_slice(&offset.to_le_bytes());
        model_bytes.extend_from_slice(name);
        model_bytes.push(0);
    }
    let skin = OldSkin {
        header: OldSkinHeader::new(),
        indices: Vec::new(),
        triangles: Vec::new(),
        bone_indices: Vec::new(),
        submeshes: Vec::new(),
        batches: Vec::new(),
    };
    let mut skin_bytes = Cursor::new(Vec::new());
    skin.write(&mut skin_bytes)?;
    let fixture = ClientFixture::with_common_files(&[
        ("Interface/Glues/Backdrop.m2", model_bytes),
        ("Interface/Glues/Backdrop00.skin", skin_bytes.get_ref()),
        (
            "Interface/Glues/Backdrop.blp",
            &support::bootstrap_texture_blp(),
        ),
    ])?;
    let mut loader = loader(&fixture)?;
    let cpu = cpu()?;
    let path = AssetPath::new("Interface/Glues/Backdrop.m2")?;
    let other = AssetPath::new("Interface/Glues/Other.m2")?;
    assert!(matches!(loader.poll(&path, &cpu), Ok(None)));
    assert!(loader.load_blocking(&other, &cpu).is_err());
    let loaded = loader.load_blocking(&path, &cpu)?;
    let reused = loader
        .poll(&path, &cpu)?
        .ok_or("ready model was deferred")?;
    assert!(Arc::ptr_eq(&loaded, &reused));
    assert!(Arc::ptr_eq(&loaded.model, &reused.model));
    assert_eq!(loaded.model.path(), &path);
    assert!(matches!(
        &loaded.textures[..],
        [
            GlueM2Texture::Authored(_),
            GlueM2Texture::StockFailure,
            GlueM2Texture::StockWhite
        ]
    ));
    let (GlueM2Texture::Authored(original_texture), GlueM2Texture::Authored(reused_texture)) =
        (&loaded.textures[0], &reused.textures[0])
    else {
        return Err("authored texture was lost".into());
    };
    assert!(Arc::ptr_eq(original_texture, reused_texture));
    assert!(!loader.has_pending());
    Ok(())
}
