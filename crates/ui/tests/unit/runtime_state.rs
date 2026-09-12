//! Runtime snapshots and retained geometry behavior.

use std::error::Error;

use super::{UiRuntimeAnchor, UiRuntimeObject, UiRuntimeObjectPlan, snapshot_text};
use crate::script::simple_script::{
    draw_layer_key, draw_sub_level_key, font_face_key, font_flags_key, font_height_key,
    font_object_key, font_set_key, font_shadow_color_key, font_shadow_offset_key, justify_h_key,
    justify_v_key, max_text_lines_key, non_space_wrap_key, spacing_key, text_color_key, text_key,
    texture_color_key, vertex_color_set_key, word_wrap_key,
};
use crate::{UiObjectKind, UiObjectRole, UiPoint};
use mlua::Lua;

fn geometry_object(parent: Option<usize>, first_anchor: usize) -> UiRuntimeObject {
    UiRuntimeObject {
        name: None,
        kind: UiObjectKind::Frame,
        role: UiObjectRole::Object,
        parent,
        width: 40.0,
        height: 20.0,
        shown: true,
        alpha: 1.0,
        scale: 1.0,
        clamp_insets: None,
        animation_alpha_delta: 0.0,
        animation_offset: (0.0, 0.0),
        animation_active: false,
        first_anchor,
        anchor_count: 1,
        texture: None,
        text: None,
        simple_html_text: None,
        model: None,
        minimap: None,
        backdrop_color: None,
        backdrop_border_color: None,
        frame_level: None,
        frame_strata: None,
        keyboard_enabled: None,
        mouse_enabled: None,
        motion_scripts_while_disabled: None,
        mouse_wheel_enabled: None,
        hit_rect_insets: None,
        scroll_offset: None,
        scroll_range: None,
        scroll_child: None,
        slider: None,
        status_bar: None,
        enabled: None,
        click_action: None,
        checked: None,
        highlighted: None,
        pushed: None,
        edit_focused: None,
    }
}

#[test]
fn retained_geometry_borrows_seeds_and_preserves_dependency_results() -> Result<(), Box<dyn Error>>
{
    // Dependencies deliberately run both forwards and backwards in arena
    // order, and include anchors across otherwise unrelated parent trees.
    let mut live = UiRuntimeObjectPlan {
        objects: [None, None, Some(4), Some(2), None, None]
            .into_iter()
            .enumerate()
            .map(|(index, parent)| geometry_object(parent, index))
            .collect(),
        anchors: [Some(4), None, Some(4), Some(2), Some(1), Some(1)]
            .into_iter()
            .map(|target| UiRuntimeAnchor {
                point: UiPoint::BottomLeft,
                target,
                relative_point: UiPoint::TopRight,
                offset: (3.0, 5.0),
            })
            .collect(),
    };
    let mut geometry = crate::UiRegionGeometryPlan::resolve(&live, (800.0, 600.0))?;
    for step in 0..3 {
        let previous = geometry.clone();
        live.objects[4].width += 13.0;
        live.objects[4].scale = 0.75;
        live.objects[4].alpha = 0.6;
        live.objects[4].animation_offset = (7.0, 11.0);
        live.objects[4].animation_active = true;
        live.anchors[4].target = Some(if step == 1 { 5 } else { 1 });
        live.objects[4].shown = step != 1;
        let refresh = geometry.refresh_dependency_regions(&live, [4, 4])?;
        assert_eq!(refresh.affected_objects, [0, 2, 3, 4]);
        let full = crate::UiRegionGeometryPlan::resolve(&live, (800.0, 600.0))?;
        let changed = (0..live.objects.len())
            .filter(|&index| previous.region(index) != full.region(index))
            .collect::<Vec<_>>();
        assert_eq!(refresh.changed_objects, changed);
        for index in 0..live.objects.len() {
            assert_eq!(geometry.region(index), full.region(index), "object {index}");
        }
    }
    let unchanged = geometry.refresh_dependency_regions(&live, [4])?;
    assert_eq!(unchanged.affected_objects, [0, 2, 3, 4]);
    assert!(unchanged.changed_objects.is_empty());
    let empty = geometry.refresh_dependency_regions(&live, [])?;
    assert!(empty.affected_objects.is_empty() && empty.changed_objects.is_empty());

    let previous = geometry.clone();
    live.anchors[4].target = Some(0);
    assert!(geometry.refresh_dependency_regions(&live, [4]).is_err());
    assert!(crate::UiRegionGeometryPlan::resolve(&live, (800.0, 600.0)).is_err());
    for index in 0..live.objects.len() {
        assert_eq!(geometry.region(index), previous.region(index));
    }
    assert!(geometry.refresh_dependency_regions(&live, [99]).is_err());
    Ok(())
}

/// Retargeting changes future invalidation; rejected cycles preserve ownership.
#[test]
fn retained_geometry_retargets_reverse_links_transactionally() -> Result<(), Box<dyn Error>> {
    let mut live = UiRuntimeObjectPlan {
        objects: (0..4096)
            .map(|index| geometry_object(None, index))
            .collect(),
        anchors: (0..4096)
            .map(|_| UiRuntimeAnchor {
                point: UiPoint::BottomLeft,
                target: None,
                relative_point: UiPoint::TopRight,
                offset: (3.0, 5.0),
            })
            .collect(),
    };
    live.anchors[3].target = Some(1);
    live.objects[4].parent = Some(3);
    let mut geometry = crate::UiRegionGeometryPlan::resolve(&live, (800., 600.))?;
    live.anchors[3].target = Some(2);
    geometry.refresh_dependency_regions(&live, [3])?;
    live.objects[1].width += 7.;
    assert_eq!(
        geometry
            .refresh_dependency_regions(&live, [1])?
            .affected_objects,
        [1]
    );
    live.objects[2].width += 11.;
    assert_eq!(
        geometry
            .refresh_dependency_regions(&live, [2])?
            .affected_objects,
        [2, 3, 4]
    );
    // Both changed dependency owners are roots even though the old graph is
    // used for discovery until this transaction commits.
    live.anchors[3].target = Some(1);
    live.objects[4].parent = Some(2);
    geometry.refresh_dependency_regions(&live, [3, 4])?;
    live.anchors[3].target = Some(3);
    assert!(geometry.refresh_dependency_regions(&live, [3]).is_err());
    live.anchors[3].target = Some(1);
    live.objects[1].scale = 0.5;
    assert_eq!(
        geometry
            .refresh_dependency_regions(&live, [1])?
            .affected_objects,
        [1, 3]
    );
    let full = crate::UiRegionGeometryPlan::resolve(&live, (800., 600.))?;
    for index in 0..live.objects.len() {
        assert_eq!(geometry.region(index), full.region(index), "object {index}");
    }
    Ok(())
}

#[test]
fn font_string_snapshot_uses_live_and_explicit_region_colors() -> Result<(), Box<dyn Error>> {
    let lua = Lua::new();
    let font = lua.create_table()?;
    font.raw_set(font_face_key(), "Fonts\\FRIZQT__.TTF")?;
    font.raw_set(font_height_key(), 10.0)?;
    font.raw_set(font_flags_key(), "")?;
    font.raw_set(
        text_color_key(),
        lua.create_sequence_from([1.0, 0.82, 0.0, 1.0])?,
    )?;
    font.raw_set(
        font_shadow_offset_key(),
        lua.create_sequence_from([0.0, 0.0])?,
    )?;
    font.raw_set(
        font_shadow_color_key(),
        lua.create_sequence_from([0.0, 0.0, 0.0, 1.0])?,
    )?;

    let object = lua.create_table()?;
    object.raw_set(font_set_key(), true)?;
    object.raw_set(font_object_key(), font)?;
    object.raw_set(text_key(), "High")?;
    object.raw_set(justify_h_key(), "CENTER")?;
    object.raw_set(justify_v_key(), "MIDDLE")?;
    object.raw_set(draw_layer_key(), "ARTWORK")?;
    object.raw_set(draw_sub_level_key(), 0_i16)?;
    object.raw_set(spacing_key(), 0.0)?;
    object.raw_set(word_wrap_key(), true)?;
    object.raw_set(non_space_wrap_key(), false)?;
    object.raw_set(max_text_lines_key(), 0_u32)?;
    object.raw_set(
        text_color_key(),
        lua.create_sequence_from([0.25, 0.5, 0.75, 0.8])?,
    )?;
    object.raw_set(
        font_shadow_offset_key(),
        lua.create_sequence_from([1.0, -1.0])?,
    )?;
    object.raw_set(
        font_shadow_color_key(),
        lua.create_sequence_from([0.1, 0.2, 0.3, 0.4])?,
    )?;
    object.raw_set(
        texture_color_key(),
        lua.create_sequence_from([1.0, 1.0, 1.0, 1.0])?,
    )?;
    object.raw_set(vertex_color_set_key(), false)?;

    let live = snapshot_text(1, UiObjectKind::FontString, &object, None, None)?
        .ok_or("test font string did not snapshot")?;
    assert_eq!(live.color, [0.25, 0.5, 0.75, 0.8]);
    assert_eq!(live.shadow_offset, [1.0, -1.0]);
    assert_eq!(live.shadow_color, [0.1, 0.2, 0.3, 0.4]);

    object.raw_set(vertex_color_set_key(), true)?;
    let live = snapshot_text(1, UiObjectKind::FontString, &object, None, None)?
        .ok_or("test font string did not snapshot")?;
    assert_eq!(live.color, [1.0; 4]);
    Ok(())
}
