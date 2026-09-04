//! Deterministic render packets derived from the post-Lua Glue object arena.

use solarity_asset::AssetPath;

use crate::script::{
    UiRuntimeModelLight, UiRuntimeModelLightSets, UiRuntimeObject, UiRuntimeObjectPlan,
};
use crate::widget::nearest_owning_scroll_frame;
use crate::{
    UiBackdropState, UiBackdropStatePlan, UiBlendMode, UiDrawLayer, UiFrameStrata, UiObjectRole,
    UiRegionGeometryPlan, UiScreenRect,
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
    pub(crate) fn for_text(live: &UiRuntimeObjectPlan, object_index: usize) -> Option<Self> {
        let object = live.objects().get(object_index)?;
        let text = object.text.as_ref()?;
        let owner_index = object
            .frame_level
            .is_some()
            .then_some(object_index)
            .or_else(|| nearest_owning_frame(live, object))?;
        let owner = live.objects().get(owner_index)?;
        Some(Self {
            strata: owner.frame_strata?,
            frame_level: owner.frame_level?,
            frame_sequence: owner_index,
            draw_rank: draw_rank(text.draw_layer, object.role),
            draw_sub_level: text.draw_sub_level,
        })
    }

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
    key: UiPresentationPacketKey,
    object_index: usize,
    /// Assigned ScrollFrame viewport, excluding sibling scrollbar chrome.
    clip_object: Option<usize>,
    /// Native Slider owning an unanchored, transform-only thumb texture.
    slider_object: Option<usize>,
    source: UiTextureSource,
    blend_mode: UiBlendMode,
    bounds: UiScreenRect,
    tex_coords: [f32; 8],
    vertex_colors: [[f32; 4]; 4],
    opacity: f32,
    horizontal_tiling: bool,
    vertical_tiling: bool,
    non_blocking: bool,
    desaturated: bool,
}

/// One visible model frame after startup Lua selected its M2 and camera state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiModelFog {
    color: [f32; 3],
    near: f32,
    far: f32,
}

impl UiModelFog {
    /// Returns the authored linear fog color.
    #[must_use]
    pub const fn color(self) -> [f32; 3] {
        self.color
    }

    /// Returns the authored near and far distances.
    #[must_use]
    pub const fn range(self) -> [f32; 2] {
        [self.near, self.far]
    }
}

/// One directional light retained from the stock flattened ModelFFX ABI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiModelLight {
    direction: [f32; 3],
    ambient: [f32; 3],
    diffuse: [f32; 3],
}

impl UiModelLight {
    /// Returns the vector from a model vertex toward the directional light.
    #[must_use]
    pub const fn direction(self) -> [f32; 3] {
        self.direction
    }

    /// Returns ambient RGB after applying the authored intensity.
    #[must_use]
    pub const fn ambient(self) -> [f32; 3] {
        self.ambient
    }

    /// Returns diffuse RGB after applying the authored intensity.
    #[must_use]
    pub const fn diffuse(self) -> [f32; 3] {
        self.diffuse
    }
}

/// Fixed live/ghost light sets matching the stock four-light capacity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiModelLightSets {
    live: [Option<UiModelLight>; 4],
    ghost: [Option<UiModelLight>; 4],
}

impl UiModelLightSets {
    /// Returns live-character lights in authored insertion order.
    #[must_use]
    pub const fn live(self) -> [Option<UiModelLight>; 4] {
        self.live
    }

    /// Returns ghost-character lights in authored insertion order.
    #[must_use]
    pub const fn ghost(self) -> [Option<UiModelLight>; 4] {
        self.ghost
    }
}

/// One visible model frame after startup Lua selected its M2 and camera state.
#[derive(Clone, Debug, PartialEq)]
pub struct UiModelPresentation {
    object_index: usize,
    path: AssetPath,
    camera: i32,
    sequence: u32,
    sequence_time_sequence: u32,
    sequence_time_ms: i32,
    model_scale: f32,
    fog: Option<UiModelFog>,
    glow: f32,
    background_lights: UiModelLightSets,
    character_lights: UiModelLightSets,
    pet_lights: UiModelLightSets,
    bounds: UiScreenRect,
    alpha: f32,
    strata: UiFrameStrata,
    frame_level: i32,
}

impl UiModelPresentation {
    /// Returns the live object-arena index used for subsequent state updates.
    #[must_use]
    pub const fn object_index(&self) -> usize {
        self.object_index
    }

    /// Returns the exact archive-backed M2 selected through `SetModel`.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the signed model-camera slot selected through `SetCamera`.
    #[must_use]
    pub const fn camera(&self) -> i32 {
        self.camera
    }

    /// Returns the base animation selected through `SetSequence`.
    #[must_use]
    pub const fn sequence(&self) -> u32 {
        self.sequence
    }

    /// Returns the sequence targeted by the most recent `SetSequenceTime`.
    #[must_use]
    pub const fn sequence_time_sequence(&self) -> u32 {
        self.sequence_time_sequence
    }

    /// Returns the authored signed time from the most recent `SetSequenceTime`.
    #[must_use]
    pub const fn sequence_time_ms(&self) -> i32 {
        self.sequence_time_ms
    }

    /// Returns the model-local scale selected independently of region scale.
    #[must_use]
    pub const fn model_scale(&self) -> f32 {
        self.model_scale
    }

    /// Returns the authored fog state, or `None` after `ClearFog`.
    #[must_use]
    pub const fn fog(&self) -> Option<UiModelFog> {
        self.fog
    }

    /// Returns the authored model glow scalar.
    #[must_use]
    pub const fn glow(&self) -> f32 {
        self.glow
    }

    /// Returns background-model live and ghost light overrides.
    #[must_use]
    pub const fn background_lights(&self) -> UiModelLightSets {
        self.background_lights
    }

    /// Returns character-model live and ghost light overrides.
    #[must_use]
    pub const fn character_lights(&self) -> UiModelLightSets {
        self.character_lights
    }

    /// Returns pet-model live and ghost light overrides.
    #[must_use]
    pub const fn pet_lights(&self) -> UiModelLightSets {
        self.pet_lights
    }

    /// Returns the visible screen rectangle occupied by the model viewport.
    #[must_use]
    pub const fn bounds(&self) -> UiScreenRect {
        self.bounds
    }

    /// Returns effective frame alpha after parent composition.
    #[must_use]
    pub const fn alpha(&self) -> f32 {
        self.alpha
    }

    /// Returns the owning model frame's presentation stratum.
    #[must_use]
    pub const fn strata(&self) -> UiFrameStrata {
        self.strata
    }

    /// Returns the owning model frame's presentation level.
    #[must_use]
    pub const fn frame_level(&self) -> i32 {
        self.frame_level
    }
}

impl UiTexturePresentation {
    pub(crate) const fn key(&self) -> UiPresentationPacketKey {
        self.key
    }

    pub(crate) const fn clip_object(&self) -> Option<usize> {
        self.clip_object
    }

    pub(crate) const fn slider_object(&self) -> Option<usize> {
        self.slider_object
    }

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

    /// Returns authored corner colors before inherited region opacity.
    #[must_use]
    pub const fn vertex_colors(&self) -> [[f32; 4]; 4] {
        self.vertex_colors
    }

    /// Returns effective region opacity after parent composition.
    #[must_use]
    pub const fn opacity(&self) -> f32 {
        self.opacity
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
    models: Vec<UiModelPresentation>,
    member_indices_by_object: Vec<Vec<usize>>,
    model_index_by_object: Vec<Option<usize>>,
    disabled_texture_owners: Vec<bool>,
}

impl UiPresentationPlan {
    /// Moves every presentation quad owned by one transform-only region.
    pub(crate) fn translate_object(&mut self, object_index: usize, delta: [f32; 2]) {
        if let Some(member_indices) = self.member_indices_by_object.get(object_index) {
            for &member_index in member_indices {
                self.members[member_index].bounds =
                    self.members[member_index].bounds.translated(delta);
            }
        }
        if let Some(Some(model_index)) = self.model_index_by_object.get(object_index) {
            self.models[*model_index].bounds = self.models[*model_index].bounds.translated(delta);
        }
    }

    /// Patches bounds and inherited opacity for one retained visual slot.
    pub(crate) fn refresh_visual_object(
        &mut self,
        live: &UiRuntimeObjectPlan,
        geometry: &UiRegionGeometryPlan,
        object_index: usize,
        translation: [f32; 2],
    ) {
        self.translate_object(object_index, translation);
        let Some(object) = live.objects().get(object_index) else {
            return;
        };
        let Some(region) = geometry.region(object_index) else {
            return;
        };
        let mut opacity = region.effective_alpha() as f32;
        if object.texture.is_some()
            && let Some(owner_index) = nearest_owning_frame(live, object)
            && let Some(owner) = live.objects().get(owner_index)
        {
            opacity *= f32::from(widget_role_is_active(
                object.role,
                owner,
                self.disabled_texture_owners[owner_index],
            ));
        }
        if let Some(member_indices) = self.member_indices_by_object.get(object_index) {
            for &member_index in member_indices {
                self.members[member_index].opacity = opacity;
            }
        }
        if let Some(Some(model_index)) = self.model_index_by_object.get(object_index) {
            self.models[*model_index].alpha = region.effective_alpha() as f32;
        }
    }

    pub(crate) fn object_opacity(&self, object_index: usize) -> Option<f32> {
        self.member_indices_by_object
            .get(object_index)
            .and_then(|indices| indices.first())
            .map(|&index| self.members[index].opacity)
    }

    /// Selects among retained Button state skins without changing draw topology.
    pub(crate) fn refresh_button_state_opacities(
        &mut self,
        live: &UiRuntimeObjectPlan,
        geometry: &UiRegionGeometryPlan,
    ) {
        let disabled_texture_owners = disabled_texture_owners(live);
        for member in &mut self.members {
            let object_index = member.object_index;
            let Some(object) = live.objects().get(object_index) else {
                continue;
            };
            let Some(owner_index) = nearest_owning_frame(live, object) else {
                continue;
            };
            let Some(owner) = live.objects().get(owner_index) else {
                continue;
            };
            let Some(region) = geometry.region(object_index) else {
                continue;
            };
            member.opacity = region.effective_alpha() as f32
                * f32::from(widget_role_is_active(
                    object.role,
                    owner,
                    disabled_texture_owners[owner_index],
                ));
        }
    }

    pub(crate) fn resolve(
        live: &UiRuntimeObjectPlan,
        geometry: &UiRegionGeometryPlan,
        backdrops: &UiBackdropStatePlan,
    ) -> Self {
        let mut keyed = Vec::new();
        let mut models = Vec::new();
        let disabled_texture_owners = disabled_texture_owners(live);
        for (object_index, object) in live.objects().iter().enumerate() {
            if let (Some(backdrop), Some(region), Some(strata), Some(frame_level)) = (
                backdrops.state(object_index),
                geometry.region(object_index),
                object.frame_strata,
                object.frame_level,
            ) && region.effectively_shown()
                && (region.effective_alpha() > 0.0 || region.animation_active())
            {
                append_backdrop(
                    &mut keyed,
                    backdrop,
                    BackdropPresentationContext {
                        object_index,
                        object,
                        clip_object: nearest_owning_scroll_frame(live, object_index),
                        bounds: region.presentation_bounds(),
                        effective_alpha: region.effective_alpha() as f32,
                        effective_scale: region.effective_scale(),
                        strata,
                        frame_level,
                    },
                );
            }
            if let Some(model) = Self::configured_model(live, geometry, object_index)
                && geometry.region(object_index).is_some_and(|region| {
                    region.effectively_shown()
                        && (region.effective_alpha() > 0.0 || region.animation_active())
                })
            {
                models.push(model);
            }
            let Some(texture) = &object.texture else {
                continue;
            };
            let Some(region) = geometry.region(object_index) else {
                continue;
            };
            if !region.effectively_shown()
                || region.effective_alpha() <= 0.0 && !region.animation_active()
            {
                continue;
            }
            let Some(owner_index) = nearest_owning_frame(live, object) else {
                continue;
            };
            let owner = &live.objects()[owner_index];
            let (Some(strata), Some(frame_level)) = (owner.frame_strata, owner.frame_level) else {
                continue;
            };
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
            let vertex_colors = texture
                .vertex_colors
                .map(|color| color.map(|value| value as f32));
            let effective_alpha = region.effective_alpha() as f32
                * f32::from(widget_role_is_active(
                    object.role,
                    owner,
                    disabled_texture_owners[owner_index],
                ));
            keyed.push((
                key,
                UiTexturePresentation {
                    key,
                    object_index,
                    clip_object: nearest_owning_scroll_frame(live, object_index),
                    slider_object: (object.role == UiObjectRole::ThumbTexture
                        && live.anchors_for(object).is_empty())
                    .then_some(object.parent)
                    .flatten()
                    .filter(|parent| live.objects()[*parent].slider.is_some()),
                    source,
                    blend_mode: texture.blend_mode,
                    bounds: region.presentation_bounds(),
                    tex_coords: texture.tex_coords.map(|value| value as f32),
                    vertex_colors,
                    opacity: effective_alpha,
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
        models.sort_by_key(|model| (model.strata, model.frame_level, model.object_index));
        let mut member_indices_by_object = vec![Vec::new(); live.objects().len()];
        for (member_index, member) in members.iter().enumerate() {
            member_indices_by_object[member.object_index].push(member_index);
        }
        let mut model_index_by_object = vec![None; live.objects().len()];
        for (model_index, model) in models.iter().enumerate() {
            model_index_by_object[model.object_index] = Some(model_index);
        }
        Self {
            packets,
            members,
            models,
            member_indices_by_object,
            model_index_by_object,
            disabled_texture_owners,
        }
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

    /// Returns visible model viewports in stable back-to-front frame order.
    #[must_use]
    pub fn models(&self) -> &[UiModelPresentation] {
        &self.models
    }

    /// Resolves one model widget without requiring its owning screen to be shown.
    pub(crate) fn configured_model(
        live: &UiRuntimeObjectPlan,
        geometry: &UiRegionGeometryPlan,
        object_index: usize,
    ) -> Option<UiModelPresentation> {
        let object = live.objects().get(object_index)?;
        let model = object.model.as_ref()?;
        let region = geometry.region(object_index)?;
        let path = model.file.as_ref()?;
        Some(UiModelPresentation {
            object_index,
            path: path.clone(),
            camera: model.camera,
            sequence: model.sequence,
            sequence_time_sequence: model.sequence_time_sequence,
            sequence_time_ms: model.sequence_time_ms,
            model_scale: model.scale as f32,
            fog: model.fog_color.map(|color| UiModelFog {
                color: color.map(|value| value as f32),
                near: model.fog_near as f32,
                far: model.fog_far as f32,
            }),
            glow: model.glow as f32,
            background_lights: model_light_sets(model.background_lights),
            character_lights: model_light_sets(model.character_lights),
            pet_lights: model_light_sets(model.pet_lights),
            bounds: region.presentation_bounds(),
            alpha: region.effective_alpha() as f32,
            strata: object.frame_strata?,
            frame_level: object.frame_level?,
        })
    }
}

#[derive(Clone, Copy)]
struct BackdropPresentationContext<'runtime> {
    object_index: usize,
    object: &'runtime UiRuntimeObject,
    clip_object: Option<usize>,
    bounds: UiScreenRect,
    effective_alpha: f32,
    effective_scale: f64,
    strata: UiFrameStrata,
    frame_level: i32,
}

fn model_light_sets(source: UiRuntimeModelLightSets) -> UiModelLightSets {
    UiModelLightSets {
        live: source.live.map(|light| light.map(model_light)),
        ghost: source.ghost.map(|light| light.map(model_light)),
    }
}

fn model_light(source: UiRuntimeModelLight) -> UiModelLight {
    UiModelLight {
        direction: source.direction.map(|value| value as f32),
        ambient: source.ambient.map(|value| value as f32),
        diffuse: source.diffuse.map(|value| value as f32),
    }
}

fn append_backdrop(
    output: &mut Vec<(UiPresentationPacketKey, UiTexturePresentation)>,
    backdrop: &UiBackdropState,
    context: BackdropPresentationContext<'_>,
) {
    let BackdropPresentationContext {
        object_index,
        object,
        clip_object,
        bounds,
        effective_alpha,
        effective_scale,
        strata,
        frame_level,
    } = context;
    if let Some(path) = backdrop.background() {
        let [left, right, top, bottom] = backdrop
            .insets()
            .map(|value| f64::from(value) * effective_scale);
        let background_bounds = UiScreenRect::from_edges(
            bounds.left() + left,
            bounds.bottom() + bottom,
            bounds.right() - right,
            bounds.top() - top,
        );
        if background_bounds.width() > 0.0 && background_bounds.height() > 0.0 {
            let mut coords = [0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0];
            if backdrop.tiled() {
                let tile_size = f64::from(backdrop.tile_size()) * effective_scale;
                let u = (background_bounds.width() / tile_size) as f32;
                let v = (background_bounds.height() / tile_size) as f32;
                coords = [0.0, 0.0, 0.0, v, u, 0.0, u, v];
            }
            push_backdrop_quad(
                output,
                object_index,
                clip_object,
                path,
                backdrop.blend_mode(),
                background_bounds,
                coords,
                runtime_backdrop_color(object.backdrop_color, backdrop.color()),
                effective_alpha,
                backdrop.tiled(),
                backdrop.tiled(),
                strata,
                frame_level,
                UiDrawLayer::Background,
            );
        }
    }

    let Some(path) = backdrop.edge() else {
        return;
    };
    let edge = (f64::from(backdrop.edge_size()) * effective_scale)
        .min(bounds.width() * 0.5)
        .min(bounds.height() * 0.5);
    if edge <= 0.0 {
        return;
    }
    let color = runtime_backdrop_color(object.backdrop_border_color, backdrop.border_color());
    let corners = [
        (
            4,
            UiScreenRect::from_edges(
                bounds.left(),
                bounds.top() - edge,
                bounds.left() + edge,
                bounds.top(),
            ),
        ),
        (
            5,
            UiScreenRect::from_edges(
                bounds.right() - edge,
                bounds.top() - edge,
                bounds.right(),
                bounds.top(),
            ),
        ),
        (
            6,
            UiScreenRect::from_edges(
                bounds.left(),
                bounds.bottom(),
                bounds.left() + edge,
                bounds.bottom() + edge,
            ),
        ),
        (
            7,
            UiScreenRect::from_edges(
                bounds.right() - edge,
                bounds.bottom(),
                bounds.right(),
                bounds.bottom() + edge,
            ),
        ),
    ];
    for (slice, corner_bounds) in corners {
        push_backdrop_quad(
            output,
            object_index,
            clip_object,
            path,
            backdrop.blend_mode(),
            corner_bounds,
            atlas_coords(slice, 1.0),
            color,
            effective_alpha,
            false,
            false,
            strata,
            frame_level,
            UiDrawLayer::Border,
        );
    }

    append_vertical_edge(
        output,
        object_index,
        clip_object,
        path,
        backdrop,
        color,
        effective_alpha,
        strata,
        frame_level,
        0,
        bounds.left(),
        bounds.bottom() + edge,
        bounds.top() - edge,
        edge,
    );
    append_vertical_edge(
        output,
        object_index,
        clip_object,
        path,
        backdrop,
        color,
        effective_alpha,
        strata,
        frame_level,
        1,
        bounds.right() - edge,
        bounds.bottom() + edge,
        bounds.top() - edge,
        edge,
    );
    append_horizontal_edge(
        output,
        object_index,
        clip_object,
        path,
        backdrop,
        color,
        effective_alpha,
        strata,
        frame_level,
        2,
        bounds.left() + edge,
        bounds.right() - edge,
        bounds.top() - edge,
        edge,
    );
    append_horizontal_edge(
        output,
        object_index,
        clip_object,
        path,
        backdrop,
        color,
        effective_alpha,
        strata,
        frame_level,
        3,
        bounds.left() + edge,
        bounds.right() - edge,
        bounds.bottom(),
        edge,
    );
}

#[allow(clippy::too_many_arguments)]
fn append_vertical_edge(
    output: &mut Vec<(UiPresentationPacketKey, UiTexturePresentation)>,
    object_index: usize,
    clip_object: Option<usize>,
    path: &AssetPath,
    backdrop: &UiBackdropState,
    color: [[f32; 4]; 4],
    opacity: f32,
    strata: UiFrameStrata,
    frame_level: i32,
    slice: u8,
    left: f64,
    bottom: f64,
    top: f64,
    edge: f64,
) {
    let mut cursor = top;
    while cursor > bottom {
        let length = (cursor - bottom).min(edge);
        let next = cursor - length;
        push_backdrop_quad(
            output,
            object_index,
            clip_object,
            path,
            backdrop.blend_mode(),
            UiScreenRect::from_edges(left, next, left + edge, cursor),
            atlas_coords(slice, (length / edge) as f32),
            color,
            opacity,
            false,
            false,
            strata,
            frame_level,
            UiDrawLayer::Border,
        );
        cursor = next;
    }
}

#[allow(clippy::too_many_arguments)]
fn append_horizontal_edge(
    output: &mut Vec<(UiPresentationPacketKey, UiTexturePresentation)>,
    object_index: usize,
    clip_object: Option<usize>,
    path: &AssetPath,
    backdrop: &UiBackdropState,
    color: [[f32; 4]; 4],
    opacity: f32,
    strata: UiFrameStrata,
    frame_level: i32,
    slice: u8,
    left: f64,
    right: f64,
    bottom: f64,
    edge: f64,
) {
    let mut cursor = left;
    while cursor < right {
        let length = (right - cursor).min(edge);
        let fraction = (length / edge) as f32;
        let u0 = f32::from(slice) / 8.0;
        let u1 = f32::from(slice + 1) / 8.0;
        // Edge slices two and three are authored vertically. Stock rotates
        // both so the atlas's outside edge faces the frame exterior.
        let coords = [u0, 1.0, u1, 1.0, u0, 1.0 - fraction, u1, 1.0 - fraction];
        push_backdrop_quad(
            output,
            object_index,
            clip_object,
            path,
            backdrop.blend_mode(),
            UiScreenRect::from_edges(cursor, bottom, cursor + length, bottom + edge),
            coords,
            color,
            opacity,
            false,
            false,
            strata,
            frame_level,
            UiDrawLayer::Border,
        );
        cursor += length;
    }
}

fn atlas_coords(slice: u8, fraction: f32) -> [f32; 8] {
    let left = f32::from(slice) / 8.0;
    let right = f32::from(slice + 1) / 8.0;
    [left, 0.0, left, fraction, right, 0.0, right, fraction]
}

fn runtime_backdrop_color(runtime: Option<[f64; 4]>, authored: [f32; 4]) -> [[f32; 4]; 4] {
    let color = runtime.map_or(authored, |color| color.map(|value| value as f32));
    [color; 4]
}

#[allow(clippy::too_many_arguments)]
fn push_backdrop_quad(
    output: &mut Vec<(UiPresentationPacketKey, UiTexturePresentation)>,
    object_index: usize,
    clip_object: Option<usize>,
    path: &AssetPath,
    blend_mode: UiBlendMode,
    bounds: UiScreenRect,
    tex_coords: [f32; 8],
    vertex_colors: [[f32; 4]; 4],
    opacity: f32,
    horizontal_tiling: bool,
    vertical_tiling: bool,
    strata: UiFrameStrata,
    frame_level: i32,
    draw_layer: UiDrawLayer,
) {
    let key = UiPresentationPacketKey {
        strata,
        frame_level,
        frame_sequence: object_index,
        draw_rank: draw_rank(draw_layer, UiObjectRole::Object),
        draw_sub_level: 0,
    };
    output.push((
        key,
        UiTexturePresentation {
            key,
            object_index,
            clip_object,
            slider_object: None,
            source: UiTextureSource::Asset(path.clone()),
            blend_mode,
            bounds,
            tex_coords,
            vertex_colors,
            opacity,
            horizontal_tiling,
            vertical_tiling,
            non_blocking: false,
            desaturated: false,
        },
    ));
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

fn disabled_texture_owners(live: &UiRuntimeObjectPlan) -> Vec<bool> {
    let mut owners = vec![false; live.objects().len()];
    for object in live.objects() {
        if object.role == UiObjectRole::DisabledTexture
            && let Some(parent) = object.parent
            && let Some(owner) = owners.get_mut(parent)
        {
            *owner = true;
        }
    }
    owners
}

fn widget_role_is_active(
    role: UiObjectRole,
    owner: &UiRuntimeObject,
    has_disabled_texture: bool,
) -> bool {
    let enabled = owner.enabled != Some(false);
    let checked = owner.checked == Some(true);
    let pushed = owner.pushed == Some(true);
    match role {
        UiObjectRole::Object
        | UiObjectRole::ScrollChild
        | UiObjectRole::ButtonText
        | UiObjectRole::ThumbTexture => true,
        UiObjectRole::NormalTexture => !pushed && (enabled || !has_disabled_texture),
        UiObjectRole::PushedTexture => enabled && pushed,
        UiObjectRole::DisabledTexture => !enabled && !checked,
        UiObjectRole::HighlightTexture => owner.highlighted == Some(true),
        UiObjectRole::CheckedTexture => enabled && checked,
        UiObjectRole::DisabledCheckedTexture => !enabled && checked,
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
        // Stock button labels sit above every mutually exclusive state skin.
        // The nested XML order is not a draw-order substitute: ButtonText is
        // usually constructed before NormalTexture in Glue templates.
        UiObjectRole::ButtonText => base + 2,
        UiObjectRole::HighlightTexture => 40,
        UiObjectRole::CheckedTexture | UiObjectRole::DisabledCheckedTexture => 41,
        _ => base,
    }
}
