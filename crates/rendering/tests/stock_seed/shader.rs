//! External stock-compatibility tests for renderer shader translation.

use std::error::Error;

use solarity_asset::{M2BlendMode, WorldModelShader};
use solarity_rendering::{
    GlowShaderPass, GlowSpirvCompiler, M2BlendFactor, M2MaterialState, M2ParticleSpirvCompiler,
    TerrainLayerCount, TerrainSpirvCompiler, UiShaderSource, UiSpirvCompiler,
    WorldModelSpirvCompiler, WorldModelSpirvKey,
};

/// Particle blend bytes and authored flags synthesize stock's dedicated material state.
#[test]
fn particle_material_uses_authored_particle_mapping() {
    let mappings = [
        (0, M2BlendMode::Opaque),
        (1, M2BlendMode::AlphaKey),
        (2, M2BlendMode::Alpha),
        (3, M2BlendMode::NoAlphaAdd),
        (4, M2BlendMode::Add),
        (5, M2BlendMode::Mod),
        (10, M2BlendMode::NoAlphaAdd),
        (u8::MAX, M2BlendMode::Opaque),
    ];
    for (selector, expected) in mappings {
        assert_eq!(
            M2MaterialState::from_particle(selector, 0).blend_mode(),
            expected
        );
    }

    let material = M2MaterialState::from_particle(4, 0x7);
    assert!(material.blend_enabled());
    assert_eq!(material.source_blend(), M2BlendFactor::SourceAlpha);
    assert_eq!(material.destination_blend(), M2BlendFactor::One);
    assert!(!material.cull_enabled());
    assert!(material.depth_test_enabled());
    assert!(!material.depth_write_enabled());
    assert!(material.is_unlit());
    assert!(!material.is_unfogged());

    let material = M2MaterialState::from_particle(5, 0x8);
    assert!(!material.is_unlit());
    assert!(material.is_unfogged());
    assert!(!material.depth_write_enabled());

    let opaque = M2MaterialState::from_particle(0, 0x4);
    assert!(opaque.depth_write_enabled());
    assert!(!opaque.is_unfogged());

    let later_unfogged = M2MaterialState::from_particle(2, 0x0010_0000);
    assert!(later_unfogged.is_unfogged());
}

/// Every stock particle blend mapping compiles for the pinned SPIR-V target.
#[test]
fn particle_shader_variants_compile_for_pinned_target() -> Result<(), Box<dyn Error>> {
    let compiler = M2ParticleSpirvCompiler::new()?;
    for selector in [0, 1, 2, 3, 4, 5, 10] {
        let material = M2MaterialState::from_particle(selector, 0);
        let program = compiler.compile(material)?;
        assert_eq!(program.material(), material);
        assert_eq!(program.vertex_words()[0], SPIRV_MAGIC);
        assert_eq!(program.vertex_words()[1], SPIRV_VERSION_1_6);
        assert_eq!(program.fragment_words()[0], SPIRV_MAGIC);
        assert_eq!(program.fragment_words()[1], SPIRV_VERSION_1_6);
    }
    Ok(())
}

const SPIRV_MAGIC: u32 = 0x0723_0203;
const SPIRV_VERSION_1_6: u32 = 0x0001_0600;

/// Every fixed FFXGlow stage compiles for the renderer's pinned target.
#[test]
fn glow_shader_passes_compile_for_pinned_target() -> Result<(), Box<dyn Error>> {
    let compiler = GlowSpirvCompiler::new()?;
    for pass in [
        GlowShaderPass::Composite,
        GlowShaderPass::Blur,
        GlowShaderPass::Box,
        GlowShaderPass::Ghost,
        GlowShaderPass::World,
        GlowShaderPass::NetherBlur,
        GlowShaderPass::NetherCombine,
    ] {
        let program = compiler.compile(pass)?;
        assert_eq!(program.pass(), pass);
        assert_eq!(program.vertex_words()[0], SPIRV_MAGIC);
        assert_eq!(program.vertex_words()[1], SPIRV_VERSION_1_6);
        assert_eq!(program.fragment_words()[0], SPIRV_MAGIC);
        assert_eq!(program.fragment_words()[1], SPIRV_VERSION_1_6);
    }
    Ok(())
}

/// All simple-render source paths compile for the pinned SPIR-V 1.6 target.
#[test]
fn ui_shader_variants_compile_for_pinned_target() -> Result<(), Box<dyn Error>> {
    let compiler = UiSpirvCompiler::new()?;
    for source in [
        UiShaderSource::Texture,
        UiShaderSource::MaskedTexture,
        UiShaderSource::VertexColor,
    ] {
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

/// Every non-null ordinary/unified MapObj effect compiles for SPIR-V 1.6.
#[test]
fn world_model_effects_compile_for_pinned_target() -> Result<(), Box<dyn Error>> {
    let compiler = WorldModelSpirvCompiler::new()?;
    let shaders = [
        WorldModelShader::Diffuse,
        WorldModelShader::Specular,
        WorldModelShader::Metal,
        WorldModelShader::Environment,
        WorldModelShader::Opaque,
        WorldModelShader::EnvironmentMetal,
        WorldModelShader::Composite,
    ];
    for unified in [false, true] {
        for shader in shaders {
            let key = WorldModelSpirvKey::new(shader, unified);
            if !unified && shader == WorldModelShader::Composite {
                assert!(key.is_err());
                continue;
            }
            let key = key?;
            let program = compiler.compile(key)?;
            assert_eq!(program.key(), key);
            assert_eq!(program.vertex_words()[0], SPIRV_MAGIC);
            assert_eq!(program.vertex_words()[1], SPIRV_VERSION_1_6);
            assert_eq!(program.fragment_words()[0], SPIRV_MAGIC);
            assert_eq!(program.fragment_words()[1], SPIRV_VERSION_1_6);
        }
    }
    Ok(())
}
