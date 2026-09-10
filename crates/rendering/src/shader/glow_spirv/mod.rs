//! Selection of build-generated SPIR-V for the stock FFXGlow pass chain.

use thiserror::Error;

const VERTEX: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/glow.vert.spv"));
const COMPOSITE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/glow-composite.frag.spv"));
const BLUR: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/glow-blur.frag.spv"));
const BOX: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/glow-box.frag.spv"));
const GHOST: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/glow-ghost.frag.spv"));
const WORLD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/glow-world.frag.spv"));
const NETHER_VERTEX: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/glow-nether.vert.spv"));
const NETHER_BLUR: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/glow-nether-blur.frag.spv"));
const NETHER_COMBINE: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/glow-nether-combine.frag.spv"));
const SPECIAL_VERTEX: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/glow-special.vert.spv"));
const SPECIAL_SEED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/glow-special-seed.frag.spv"));
const SPECIAL_PROPAGATE: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/glow-special-propagate.frag.spv"));
const SPECIAL_COMBINE: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/glow-special-combine.frag.spv"));

/// Stable identity of one fragment stage in the stock glow chain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GlowShaderPass {
    /// Full-resolution scene plus squared blurred light contribution and gamma.
    Composite,
    /// One horizontal or vertical four-tap gaussian pass.
    Blur,
    /// Four-tap full-to-quarter-resolution downsample.
    Box,
    /// Stock FFXDeath grayscale, glow and blue tint composition.
    Ghost,
    /// Ordinary world glow, player blur and the optional underwater distortion.
    World,
    /// Invisibility's animated 6x6 four-tap mesh.
    NetherBlur,
    /// Invisibility's fade and tinted scene composition.
    NetherCombine,
    /// Procedural noise strip and retained screen-filter history.
    SpecialSeed,
    /// Four-tap upward history propagation and alpha decay.
    SpecialPropagate,
    /// Polar history mesh and screen-filter desaturation/whitening.
    SpecialCombine,
}

/// Owned SPIR-V modules for one glow pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlowSpirvProgram {
    pass: GlowShaderPass,
    vertex_words: Vec<u32>,
    fragment_words: Vec<u32>,
}

impl GlowSpirvProgram {
    /// Returns the fragment operation represented by this pair.
    #[must_use]
    pub const fn pass(&self) -> GlowShaderPass {
        self.pass
    }

    /// Returns the fullscreen-triangle vertex module.
    #[must_use]
    pub fn vertex_words(&self) -> &[u32] {
        &self.vertex_words
    }

    /// Returns the selected glow fragment module.
    #[must_use]
    pub fn fragment_words(&self) -> &[u32] {
        &self.fragment_words
    }
}

/// Build-generated glow bytecode could not be selected.
#[derive(Debug, Error)]
#[error("build-generated glow SPIR-V is unavailable")]
pub struct GlowSpirvError;

/// Selector for the fixed build-generated glow pipeline pairs.
pub struct GlowSpirvCompiler;

impl GlowSpirvCompiler {
    /// Creates the build-generated shader selector.
    pub fn new() -> Result<Self, GlowSpirvError> {
        Ok(Self)
    }

    /// Selects one fixed pass compiled during the Cargo build.
    pub fn compile(&self, pass: GlowShaderPass) -> Result<GlowSpirvProgram, GlowSpirvError> {
        let fragment = match pass {
            GlowShaderPass::Composite => COMPOSITE,
            GlowShaderPass::Blur => BLUR,
            GlowShaderPass::Box => BOX,
            GlowShaderPass::Ghost => GHOST,
            GlowShaderPass::World => WORLD,
            GlowShaderPass::NetherBlur => NETHER_BLUR,
            GlowShaderPass::NetherCombine => NETHER_COMBINE,
            GlowShaderPass::SpecialSeed => SPECIAL_SEED,
            GlowShaderPass::SpecialPropagate => SPECIAL_PROPAGATE,
            GlowShaderPass::SpecialCombine => SPECIAL_COMBINE,
        };
        Ok(GlowSpirvProgram {
            pass,
            vertex_words: spirv_words(match pass {
                GlowShaderPass::NetherBlur => NETHER_VERTEX,
                GlowShaderPass::SpecialCombine => SPECIAL_VERTEX,
                _ => VERTEX,
            }),
            fragment_words: spirv_words(fragment),
        })
    }
}

fn spirv_words(bytes: &[u8]) -> Vec<u32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|word| u32::from_le_bytes([word[0], word[1], word[2], word[3]]))
        .collect()
}
