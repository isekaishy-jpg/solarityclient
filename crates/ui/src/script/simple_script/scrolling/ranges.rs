//! Scroll ranges are layout results, independent of repeated offset changes.

use super::super::{
    LiveRegionBounds, OBJECT_CHILDREN_REGISTRY, OBJECT_REGISTRY, horizontal_scroll_range_key,
    index_key, live_region_dimensions, resolve_live_region_bounds_inner, scroll_child_key,
    shown_key, vertical_scroll_range_key,
};
use mlua::{Lua, Table};
use std::collections::HashMap;

/// One entry per live ScrollFrame; layout generations replace earlier results.
#[derive(Default)]
struct RangeCache {
    generation: u64,
    entries: HashMap<usize, RangeEntry>,
}

/// The layout generation and viewport fully identify one cached range.
struct RangeEntry {
    generation: u64,
    extent: (f64, f64),
    range: (f64, f64),
}

/// The cache belongs to the same Lua object arena as its numeric owner keys.
pub(in super::super) fn initialize(lua: &Lua) {
    lua.set_app_data(RangeCache::default());
}

/// Layout, visibility and explicit child-rect publication invalidate this cache.
/// Offset/value changes and unrelated color writes preserve the measured range.
pub(in super::super) fn invalidate_ranges(lua: &Lua) {
    // Global registrations can mutate Lua before the object arena exists.
    if let Some(mut cache) = lua.app_data_mut::<RangeCache>() {
        cache.generation = cache.generation.wrapping_add(1);
    }
}

/// Publishes the range implied by the scroll frame's current live dimensions.
pub(in super::super) fn refresh_scroll_ranges(
    lua: &Lua,
    object: &Table,
    ui_extent: (f64, f64),
) -> mlua::Result<(f64, f64)> {
    let object_index = object.raw_get::<usize>(index_key())?;
    let cache = lua
        .app_data_ref::<RangeCache>()
        .ok_or_else(|| mlua::Error::runtime("scroll layout cache is not initialized"))?;
    let generation = cache.generation;
    if let Some(entry) = cache.entries.get(&object_index)
        && entry.generation == generation
        && entry.extent == ui_extent
    {
        return Ok(entry.range);
    }
    drop(cache);
    let (frame_width, frame_height) = live_region_dimensions(lua, object, ui_extent)?;
    let (child_width, child_height) = object
        .raw_get::<Option<Table>>(scroll_child_key())?
        .map(|child| scroll_child_content_dimensions(lua, &child, ui_extent))
        .transpose()?
        .unwrap_or((0.0, 0.0));
    let horizontal_range = (child_width - frame_width).max(0.0);
    let vertical_range = (child_height - frame_height).max(0.0);
    object.raw_set(horizontal_scroll_range_key(), horizontal_range)?;
    object.raw_set(vertical_scroll_range_key(), vertical_range)?;
    lua.app_data_mut::<RangeCache>()
        .ok_or_else(|| mlua::Error::runtime("scroll layout cache is not initialized"))?
        .entries
        .insert(
            object_index,
            RangeEntry {
                generation,
                extent: ui_extent,
                range: (horizontal_range, vertical_range),
            },
        );
    Ok((horizontal_range, vertical_range))
}

/// Returns the top-left-origin content extent of a scroll child and all shown
/// descendants. Build 12340 creation panes deliberately give the child frame
/// a ten-pixel seed height while auto-sized FontStrings extend far below it;
/// `CSimpleScrollFrame` includes those descendant region bounds in its range.
fn scroll_child_content_dimensions(
    lua: &Lua,
    child: &Table,
    ui_extent: (f64, f64),
) -> mlua::Result<(f64, f64)> {
    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    let children: Table = lua.named_registry_value(OBJECT_CHILDREN_REGISTRY)?;
    let screen = LiveRegionBounds {
        left: 0.0,
        bottom: 0.0,
        right: ui_extent.0,
        top: ui_extent.1,
    };
    let mut resolved = HashMap::new();
    let mut visiting = Vec::new();
    let Some(child_bounds) = resolve_live_region_bounds_inner(
        &objects,
        child.clone(),
        screen,
        &mut resolved,
        &mut visiting,
    )?
    else {
        return live_region_dimensions(lua, child, ui_extent);
    };
    let child_index = child.raw_get::<usize>(index_key())?;
    let mut maximum_right = child_bounds.right;
    let mut minimum_bottom = child_bounds.bottom;
    let mut pending = vec![child_index];
    while let Some(parent_index) = pending.pop() {
        let Some(direct) = children.raw_get::<Option<Table>>(parent_index)? else {
            continue;
        };
        for child_index in direct.sequence_values::<usize>() {
            let child_index = child_index?;
            let candidate: Table = objects.raw_get(child_index)?;
            if !candidate.raw_get::<bool>(shown_key())? {
                continue;
            }
            pending.push(child_index);
            if let Some(bounds) = resolve_live_region_bounds_inner(
                &objects,
                candidate,
                screen,
                &mut resolved,
                &mut visiting,
            )? {
                maximum_right = maximum_right.max(bounds.right);
                minimum_bottom = minimum_bottom.min(bounds.bottom);
            }
        }
    }
    Ok((
        (maximum_right - child_bounds.left).max(0.0),
        (child_bounds.top - minimum_bottom).max(0.0),
    ))
}
