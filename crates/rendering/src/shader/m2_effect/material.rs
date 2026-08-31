//! Stock pass-zero blend, alpha-test, culling, and depth state.

use solarity_asset::{M2BlendMode, M2Material};

/// Backend-independent blend factors used by the Vulkan pipeline cache key.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum M2BlendFactor {
    /// Numeric zero.
    Zero,
    /// Numeric one.
    One,
    /// Source fragment alpha.
    SourceAlpha,
    /// One minus source fragment alpha.
    OneMinusSourceAlpha,
    /// Existing destination color.
    DestinationColor,
    /// Incoming source color.
    SourceColor,
}

/// Immutable pass-zero material state recovered from `CM2SceneRender`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct M2MaterialState {
    blend_mode: M2BlendMode,
    blend_enabled: bool,
    source_blend: M2BlendFactor,
    destination_blend: M2BlendFactor,
    cull_enabled: bool,
    depth_test_enabled: bool,
    depth_write_enabled: bool,
    is_unlit: bool,
    is_unfogged: bool,
}

impl M2MaterialState {
    /// Converts exact M2 flags through stock's pass-zero GX state tables.
    #[must_use]
    pub const fn from_material(material: M2Material) -> Self {
        let blend_mode = material.blend_mode();
        let (blend_enabled, source_blend, destination_blend) = match blend_mode {
            M2BlendMode::Opaque | M2BlendMode::AlphaKey => {
                (false, M2BlendFactor::One, M2BlendFactor::Zero)
            }
            M2BlendMode::Alpha => (
                true,
                M2BlendFactor::SourceAlpha,
                M2BlendFactor::OneMinusSourceAlpha,
            ),
            M2BlendMode::NoAlphaAdd => (true, M2BlendFactor::One, M2BlendFactor::One),
            M2BlendMode::Add => (true, M2BlendFactor::SourceAlpha, M2BlendFactor::One),
            M2BlendMode::Mod => (true, M2BlendFactor::DestinationColor, M2BlendFactor::Zero),
            M2BlendMode::Mod2x => (
                true,
                M2BlendFactor::DestinationColor,
                M2BlendFactor::SourceColor,
            ),
        };
        let flags = material.flags();
        Self {
            blend_mode,
            blend_enabled,
            source_blend,
            destination_blend,
            cull_enabled: flags & 0x4 == 0,
            depth_test_enabled: flags & 0x8 == 0,
            depth_write_enabled: flags & 0x10 == 0,
            // Stock marks multiplicative materials unlit during M2 shared setup.
            is_unlit: flags & 0x1 != 0
                || matches!(blend_mode, M2BlendMode::Mod | M2BlendMode::Mod2x),
            is_unfogged: flags & 0x2 != 0,
        }
    }

    /// Returns stock's pipeline override for a normally opaque material whose
    /// runtime element alpha has fallen below `0.99999`.
    #[must_use]
    pub const fn with_runtime_alpha_fade(mut self) -> Self {
        self.blend_enabled = true;
        self.source_blend = M2BlendFactor::SourceAlpha;
        self.destination_blend = M2BlendFactor::OneMinusSourceAlpha;
        self.depth_write_enabled = false;
        self
    }

    /// Returns the authored blend classification.
    #[must_use]
    pub const fn blend_mode(self) -> M2BlendMode {
        self.blend_mode
    }

    /// Reports whether Vulkan blending is enabled for pass zero.
    #[must_use]
    pub const fn blend_enabled(self) -> bool {
        self.blend_enabled
    }

    /// Returns the stock source-color blend factor.
    #[must_use]
    pub const fn source_blend(self) -> M2BlendFactor {
        self.source_blend
    }

    /// Returns the stock destination-color blend factor.
    #[must_use]
    pub const fn destination_blend(self) -> M2BlendFactor {
        self.destination_blend
    }

    /// Reports whether back faces are culled.
    #[must_use]
    pub const fn cull_enabled(self) -> bool {
        self.cull_enabled
    }

    /// Reports whether depth comparison is enabled.
    #[must_use]
    pub const fn depth_test_enabled(self) -> bool {
        self.depth_test_enabled
    }

    /// Reports whether passing fragments write depth.
    #[must_use]
    pub const fn depth_write_enabled(self) -> bool {
        self.depth_write_enabled
    }

    /// Reports stock's effective unlit state after multiplicative adjustment.
    #[must_use]
    pub const fn is_unlit(self) -> bool {
        self.is_unlit
    }

    /// Reports whether model fog is disabled.
    #[must_use]
    pub const fn is_unfogged(self) -> bool {
        self.is_unfogged
    }

    /// Computes stock's shader alpha-reference constant for an instance alpha.
    #[must_use]
    pub fn alpha_reference(self, instance_alpha: f32) -> f32 {
        match self.blend_mode {
            M2BlendMode::Opaque => 0.0,
            M2BlendMode::AlphaKey => instance_alpha * (224.0 / 255.0),
            M2BlendMode::Alpha
            | M2BlendMode::NoAlphaAdd
            | M2BlendMode::Add
            | M2BlendMode::Mod
            | M2BlendMode::Mod2x => 1.0 / 255.0,
        }
    }
}
