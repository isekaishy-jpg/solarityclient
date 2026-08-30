//! External tests for build-12340 BLS shader-library decoding.

use std::error::Error;

use solarity_asset::{
    ArchiveCatalog, AssetError, AssetPath, AssetStore, BlsShaderStage, ClientDataRoot,
    DecodedBlsShader, Locale,
};

use crate::support::{Fixture, FixtureFile};

/// BLS 1.3 retains every permutation selector and Shader Model 3 token stream.
#[test]
fn bls_shader_library_preserves_stock_permutations() -> Result<(), Box<dyn Error>> {
    let shader = bls_bytes(VERTEX_TOKEN);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "Shaders\\Vertex\\vs_3_0\\Diffuse_T1.bls",
        bytes: &shader,
    }])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Shaders\\Vertex\\vs_3_0\\Diffuse_T1.bls")?;
    let shader = DecodedBlsShader::load(&mut store, &path, BlsShaderStage::Vertex)?;

    assert_eq!(shader.path(), &path);
    assert_eq!(shader.stage(), BlsShaderStage::Vertex);
    assert_eq!(shader.permutations().len(), 2);
    assert_eq!(shader.permutations()[0].flags_0(), 0x801);
    assert_eq!(shader.permutations()[0].flags_4(), 0xA81);
    assert_eq!(shader.permutations()[0].selector(), 0);
    assert_eq!(shader.permutations()[0].bytecode(), bytecode(VERTEX_TOKEN));
    assert_eq!(shader.permutations()[1].selector(), 0x10);
    Ok(())
}

/// A pixel token cannot be silently accepted as a requested vertex program.
#[test]
fn bls_shader_library_rejects_another_stage() -> Result<(), Box<dyn Error>> {
    let shader = bls_bytes(PIXEL_TOKEN);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "Shaders\\Vertex\\vs_3_0\\Wrong.bls",
        bytes: &shader,
    }])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Shaders\\Vertex\\vs_3_0\\Wrong.bls")?;

    assert!(matches!(
        DecodedBlsShader::load(&mut store, &path, BlsShaderStage::Vertex),
        Err(AssetError::ShaderDecode { .. })
    ));
    Ok(())
}

const VERTEX_TOKEN: u32 = 0xFFFE_0300;
const PIXEL_TOKEN: u32 = 0xFFFF_0300;

/// Creates two aligned BLS 1.3 permutation blocks.
fn bls_bytes(token: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"HSXG");
    bytes.extend_from_slice(&0x0001_0003_u32.to_le_bytes());
    bytes.extend_from_slice(&2_u32.to_le_bytes());
    for (flags_0, flags_4, selector) in [(0x801_u32, 0xA81_u32, 0_u32), (1, 2, 0x10)] {
        let bytecode = bytecode(token);
        bytes.extend_from_slice(&flags_0.to_le_bytes());
        bytes.extend_from_slice(&flags_4.to_le_bytes());
        bytes.extend_from_slice(&selector.to_le_bytes());
        bytes.extend_from_slice(&(bytecode.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&bytecode);
    }
    bytes
}

/// Produces the smallest token stream needed by this structural boundary.
fn bytecode(token: u32) -> Vec<u8> {
    let mut bytes = token.to_le_bytes().to_vec();
    bytes.extend_from_slice(&0x0000_FFFF_u32.to_le_bytes());
    bytes
}
