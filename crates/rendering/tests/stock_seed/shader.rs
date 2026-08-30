//! External stock-compatibility tests for renderer shader translation.

use std::error::Error;

use solarity_rendering::{
    TerrainLayerCount, TerrainSpirvCompiler, UiShaderSource, UiSpirvCompiler,
};

const SPIRV_MAGIC: u32 = 0x0723_0203;
const SPIRV_VERSION_1_6: u32 = 0x0001_0600;

/// Both simple-render source paths compile for the pinned SPIR-V 1.6 target.
#[test]
fn ui_shader_variants_compile_for_pinned_target() -> Result<(), Box<dyn Error>> {
    let compiler = UiSpirvCompiler::new()?;
    for source in [UiShaderSource::Texture, UiShaderSource::VertexColor] {
        let program = compiler.compile(source)?;
        assert_eq!(program.source(), source);
        assert_eq!(program.vertex_words()[0], SPIRV_MAGIC);
        assert_eq!(program.vertex_words()[1], SPIRV_VERSION_1_6);
        assert_eq!(program.fragment_words()[0], SPIRV_MAGIC);
        assert_eq!(program.fragment_words()[1], SPIRV_VERSION_1_6);
    }
    Ok(())
}

/// Every stock MCNK layer count compiles against the same pinned target.
#[test]
fn terrain_shader_variants_compile_for_pinned_target() -> Result<(), Box<dyn Error>> {
    let compiler = TerrainSpirvCompiler::new()?;
    for layer_count in [
        TerrainLayerCount::One,
        TerrainLayerCount::Two,
        TerrainLayerCount::Three,
        TerrainLayerCount::Four,
    ] {
        let program = compiler.compile(layer_count)?;
        assert_eq!(program.layer_count(), layer_count);
        assert_eq!(program.vertex_words()[0], SPIRV_MAGIC);
        assert_eq!(program.vertex_words()[1], SPIRV_VERSION_1_6);
        assert_eq!(program.fragment_words()[0], SPIRV_MAGIC);
        assert_eq!(program.fragment_words()[1], SPIRV_VERSION_1_6);
    }
    assert!(TerrainLayerCount::try_from(0).is_err());
    assert!(TerrainLayerCount::try_from(5).is_err());
    Ok(())
}
