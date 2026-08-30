//! MOBA class expansion and stock ordinary/unified lighting selection.

use solarity_asset::{WorldModelBatchClass, WorldModelBlendMode, WorldModelMaterial};

use super::WorldModelMaterialState;

/// Lighting constants selected by the stock MapObj submitters.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorldModelLightingMode {
    /// Retain the fixed MOCV-authored vertex contribution without world light.
    Authored,
    /// Use the map-global ambient and directional light.
    Exterior,
    /// Use stock's byte-quantized alternate exterior light pair.
    FlattenedExterior,
    /// Use MOHD ambient without a directional contribution.
    RootAmbient,
}

/// One physical submission generated from a logical MOBA draw.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WorldModelSurfacePass {
    material: WorldModelMaterialState,
    lighting: WorldModelLightingMode,
}

impl WorldModelSurfacePass {
    const fn new(material: WorldModelMaterialState, lighting: WorldModelLightingMode) -> Self {
        Self { material, lighting }
    }

    /// Returns fixed-function and shader state for this physical pass.
    #[must_use]
    pub const fn material(self) -> WorldModelMaterialState {
        self.material
    }

    /// Returns the light constants consumed by this physical pass.
    #[must_use]
    pub const fn lighting(self) -> WorldModelLightingMode {
        self.lighting
    }
}

/// Allocation-free one- or two-pass expansion of one logical MOBA surface.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WorldModelSurfacePassPlan {
    unified: bool,
    passes: [WorldModelSurfacePass; 2],
    pass_count: u8,
}

impl WorldModelSurfacePassPlan {
    /// Replays ordinary or unified MapObj surface selection.
    #[must_use]
    pub const fn prepare(
        root_flags: u16,
        group_flags: u32,
        class: WorldModelBatchClass,
        material: &WorldModelMaterial,
    ) -> Self {
        const UNIFIED_RENDERING: u16 = 0x02;

        let unified = root_flags & UNIFIED_RENDERING != 0;
        let ordinary_lighting = lighting_mode(unified, group_flags, class, material.flags());
        let ordinary = WorldModelSurfacePass::new(
            WorldModelMaterialState::from_material(material),
            ordinary_lighting,
        );
        if !matches!(class, WorldModelBatchClass::Transition) {
            return Self {
                unified,
                passes: [ordinary; 2],
                pass_count: 1,
            };
        }

        // Transition MOBA ranges are a stock cross-fade, irrespective of the
        // authored MOMT blend. Unified MapObj changes only the complement pass
        // to root ambient; the ordinary path retains the first pass light.
        let first = WorldModelSurfacePass::new(
            WorldModelMaterialState::from_material_with_blend(
                material,
                WorldModelBlendMode::SourceAlphaOpaque,
            ),
            ordinary_lighting,
        );
        let complement_lighting = if unified {
            WorldModelLightingMode::RootAmbient
        } else {
            ordinary_lighting
        };
        let second = WorldModelSurfacePass::new(
            WorldModelMaterialState::from_material_with_blend(
                material,
                WorldModelBlendMode::InverseSourceAlphaAdd,
            ),
            complement_lighting,
        );
        Self {
            unified,
            passes: [first, second],
            pass_count: 2,
        }
    }

    /// Reports whether the root selected the MapObjU effect family.
    #[must_use]
    pub const fn is_unified(self) -> bool {
        self.unified
    }

    /// Returns the exact physical pass sequence.
    #[must_use]
    pub const fn passes(&self) -> &[WorldModelSurfacePass] {
        self.passes.split_at(self.pass_count as usize).0
    }
}

const fn lighting_mode(
    unified: bool,
    group_flags: u32,
    class: WorldModelBatchClass,
    material_flags: u32,
) -> WorldModelLightingMode {
    const UNLIT: u32 = 0x01;
    const FLATTEN_LIGHTING: u32 = 0x20;
    const EXTERIOR: u32 = 0x08;
    const EXTERIOR_LIT: u32 = 0x40;

    if material_flags & UNLIT != 0 {
        return WorldModelLightingMode::Authored;
    }
    let flattened = material_flags & FLATTEN_LIGHTING != 0;
    if !unified {
        if matches!(class, WorldModelBatchClass::Interior) {
            return WorldModelLightingMode::Authored;
        }
        return if flattened {
            WorldModelLightingMode::FlattenedExterior
        } else {
            WorldModelLightingMode::Exterior
        };
    }
    if matches!(class, WorldModelBatchClass::Transition)
        || group_flags & (EXTERIOR | EXTERIOR_LIT) != 0
        || flattened
    {
        return if flattened {
            WorldModelLightingMode::FlattenedExterior
        } else {
            WorldModelLightingMode::Exterior
        };
    }
    WorldModelLightingMode::RootAmbient
}
