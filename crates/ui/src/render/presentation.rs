//! Deterministic render packets derived from the post-Lua Glue object arena.

use solarity_asset::AssetPath;

use crate::script::{UiRuntimeObject, UiRuntimeObjectPlan};
use crate::{
    UiBlendMode, UiDrawLayer, UiFrameStrata, UiObjectRole, UiRegionGeometryPlan, UiScreenRect,
};

/// A texture's renderable source after stock `SetTexture` mutation.
#[derive(Clone, Debug, PartialEq)]
pub enum UiTextureSource {
    /// One canonical MPQ-backed BLP path.
    Asset(AssetPath),
    /// A source-less quad carrying a uniform RGBA value.
    SolidColor([f32; 4]),
}

/// Exact back-to-front packet ordering recovered from stock presentation state.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct UiPresentationPacketKey {
    strata: UiFrameStrata,
    frame_level: i32,
    frame_sequence: usize,
    draw_rank: i16,
    draw_sub_level: i16,
}

impl UiPresentationPacketKey {
    /// Returns the owning frame stratum.
    #[must_use]
    pub const fn strata(self) -> UiFrameStrata {
        self.strata
    }

    /// Returns the owning frame level.
    #[must_use]
    pub const fn frame_level(self) -> i32 {
        self.frame_level
    }

    /// Returns the owning frame's stable creation-order index.
    #[must_use]
    pub const fn frame_sequence(self) -> usize {
        self.frame_sequence
    }

    /// Returns the stock draw-band rank including widget-role adjustments.
    #[must_use]
    pub const fn draw_rank(self) -> i16 {
        self.draw_rank
    }

    /// Returns the signed offset inside the draw band.
    #[must_use]
    pub const fn draw_sub_level(self) -> i16 {
        self.draw_sub_level
    }
}

/// One render-ready texture quad retaining its live object identity.
#[derive(Clone, Debug, PartialEq)]
pub struct UiTexturePresentation {
    object_index: usize,
    source: UiTextureSource,
    blend_mode: UiBlendMode,
    bounds: UiScreenRect,
    tex_coords: [f32; 8],
    vertex_colors: [[f32; 4]; 4],
    horizontal_tiling: bool,
    vertical_tiling: bool,
    non_blocking: bool,
    desaturated: bool,
}

impl UiTexturePresentation {
    /// Returns the live object-arena index used for subsequent updates.
    #[must_use]
    pub const fn object_index(&self) -> usize {
        self.object_index
    }

    /// Returns the archive or solid-color material source.
    #[must_use]
    pub const fn source(&self) -> &UiTextureSource {
        &self.source
    }

    /// Returns the final texture blend mode.
    #[must_use]
    pub const fn blend_mode(&self) -> UiBlendMode {
        self.blend_mode
    }

    /// Returns the scaled screen-space quad bounds.
    #[must_use]
    pub const fn bounds(&self) -> UiScreenRect {
        self.bounds
    }

    /// Returns upper-left, lower-left, upper-right, lower-right UV pairs.
    #[must_use]
    pub const fn tex_coords(&self) -> [f32; 8] {
        self.tex_coords
    }

    /// Returns corner colors with effective object alpha already composed.
    #[must_use]
    pub const fn vertex_colors(&self) -> [[f32; 4]; 4] {
        self.vertex_colors
    }

    /// Returns whether U coordinates wrap.
    #[must_use]
    pub const fn horizontal_tiling(&self) -> bool {
        self.horizontal_tiling
    }

    /// Returns whether V coordinates wrap.
    #[must_use]
    pub const fn vertical_tiling(&self) -> bool {
        self.vertical_tiling
    }

    /// Returns the stock asynchronous residency request.
    #[must_use]
    pub const fn non_blocking(&self) -> bool {
        self.non_blocking
    }

    /// Returns whether the texture shader converts sampled RGB to grayscale.
    #[must_use]
    pub const fn desaturated(&self) -> bool {
        self.desaturated
    }
}

/// A contiguous set of texture quads sharing one presentation key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiPresentationPacket {
    key: UiPresentationPacketKey,
    first_member: usize,
    member_count: usize,
}

impl UiPresentationPacket {
    /// Returns the complete deterministic sort key.
    #[must_use]
    pub const fn key(self) -> UiPresentationPacketKey {
        self.key
    }

    /// Returns the number of creation-ordered quads in this packet.
    #[must_use]
    pub const fn member_count(self) -> usize {
        self.member_count
    }
}

/// Compact, sorted texture packets ready for renderer-side resource binding.
pub struct UiPresentationPlan {
    packets: Vec<UiPresentationPacket>,
    members: Vec<UiTexturePresentation>,
}

impl UiPresentationPlan {
    pub(crate) fn resolve(live: &UiRuntimeObjectPlan, geometry: &UiRegionGeometryPlan) -> Self {
        let mut keyed = Vec::new();
        for (object_index, object) in live.objects().iter().enumerate() {
            let Some(texture) = &object.texture else {
                continue;
            };
            let Some(region) = geometry.region(object_index) else {
                continue;
            };
            if !region.effectively_shown() || region.effective_alpha() <= 0.0 {
                continue;
            }
            let Some(owner_index) = nearest_owning_frame(live, object) else {
                continue;
            };
            let owner = &live.objects()[owner_index];
            let (Some(strata), Some(frame_level)) = (owner.frame_strata, owner.frame_level) else {
                continue;
            };
            if !widget_role_is_presented(object.role, owner) {
                continue;
            }
            let source = if let Some(path) = &texture.file {
                UiTextureSource::Asset(path.clone())
            } else if let Some(color) = texture.solid_color {
                UiTextureSource::SolidColor(color.map(|value| value as f32))
            } else {
                continue;
            };
            let key = UiPresentationPacketKey {
                strata,
                frame_level,
                frame_sequence: owner_index,
                draw_rank: draw_rank(texture.draw_layer, object.role),
                draw_sub_level: texture.draw_sub_level,
            };
            let mut vertex_colors = texture
                .vertex_colors
                .map(|color| color.map(|value| value as f32));
            let effective_alpha = region.effective_alpha() as f32;
            for color in &mut vertex_colors {
                color[3] *= effective_alpha;
            }
            keyed.push((
                key,
                UiTexturePresentation {
                    object_index,
                    source,
                    blend_mode: texture.blend_mode,
                    bounds: region.presentation_bounds(),
                    tex_coords: texture.tex_coords.map(|value| value as f32),
                    vertex_colors,
                    horizontal_tiling: texture.horizontal_tiling,
                    vertical_tiling: texture.vertical_tiling,
                    non_blocking: texture.non_blocking,
                    desaturated: texture.desaturated,
                },
            ));
        }
        keyed.sort_by_key(|(key, member)| (*key, member.object_index));

        let mut packets = Vec::new();
        let mut members = Vec::with_capacity(keyed.len());
        for (key, member) in keyed {
            let starts_packet = packets
                .last()
                .is_none_or(|packet: &UiPresentationPacket| packet.key != key);
            if starts_packet {
                packets.push(UiPresentationPacket {
                    key,
                    first_member: members.len(),
                    member_count: 0,
                });
            }
            if let Some(packet) = packets.last_mut() {
                packet.member_count += 1;
            }
            members.push(member);
        }
        Self { packets, members }
    }

    /// Returns packets in exact back-to-front presentation order.
    #[must_use]
    pub fn packets(&self) -> &[UiPresentationPacket] {
        &self.packets
    }

    /// Returns creation-ordered members of one packet.
    #[must_use]
    pub fn members(&self, packet_index: usize) -> Option<&[UiTexturePresentation]> {
        let packet = self.packets.get(packet_index)?;
        Some(&self.members[packet.first_member..packet.first_member + packet.member_count])
    }

    /// Returns the total number of renderable texture quads.
    #[must_use]
    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    /// Returns all packet members in their final contiguous draw order.
    #[must_use]
    pub fn members_in_draw_order(&self) -> &[UiTexturePresentation] {
        &self.members
    }
}

fn nearest_owning_frame(live: &UiRuntimeObjectPlan, object: &UiRuntimeObject) -> Option<usize> {
    let mut cursor = object.parent;
    while let Some(index) = cursor {
        let candidate = live.objects().get(index)?;
        if candidate.frame_level.is_some() {
            return Some(index);
        }
        cursor = candidate.parent;
    }
    None
}

fn widget_role_is_presented(role: UiObjectRole, owner: &UiRuntimeObject) -> bool {
    match role {
        UiObjectRole::Object
        | UiObjectRole::ScrollChild
        | UiObjectRole::ButtonText
        | UiObjectRole::ThumbTexture => true,
        UiObjectRole::NormalTexture => owner.enabled != Some(false),
        UiObjectRole::PushedTexture => owner.enabled != Some(false) && owner.pushed == Some(true),
        UiObjectRole::DisabledTexture => owner.enabled == Some(false),
        UiObjectRole::HighlightTexture => owner.highlighted == Some(true),
        UiObjectRole::CheckedTexture => owner.enabled != Some(false) && owner.checked == Some(true),
        UiObjectRole::DisabledCheckedTexture => {
            owner.enabled == Some(false) && owner.checked == Some(true)
        }
    }
}

const fn draw_rank(layer: UiDrawLayer, role: UiObjectRole) -> i16 {
    let base = match layer {
        UiDrawLayer::Background => 0,
        UiDrawLayer::Border => 10,
        UiDrawLayer::Artwork => 20,
        UiDrawLayer::Overlay => 30,
        UiDrawLayer::Highlight => 40,
    };
    match role {
        UiObjectRole::PushedTexture | UiObjectRole::DisabledTexture => base + 1,
        UiObjectRole::HighlightTexture => 40,
        UiObjectRole::CheckedTexture | UiObjectRole::DisabledCheckedTexture => 41,
        _ => base,
    }
}
