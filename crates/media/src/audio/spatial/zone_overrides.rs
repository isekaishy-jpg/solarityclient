//! Native world-state precedence and sound-chunk coordinate selection.

use solarity_asset::{AreaSoundReferences, WorldChunkSoundKey, WorldStateZoneSound};

/// Location IDs passed to 4CBE70, retaining both DBC namespaces.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ZoneSoundLocationIds {
    /// AreaTable zone and optional child area, with zero for absence.
    pub areas: [u32; 2],
    /// WMOAreaTable root and group IDs, with zero for absence.
    pub world_model_areas: [u32; 2],
    /// Removes AreaTable matches inside an exclusive WMO location.
    pub world_model_only: bool,
}

/// Resolves 4CBE70's ordered world-state rules without consuming playback state.
/// Missing world-state fields must be supplied as zero by the replicated owner.
pub fn resolve_world_state_zone_sounds(
    rows: &[WorldStateZoneSound],
    location: ZoneSoundLocationIds,
    state_value: impl Fn(u32) -> u32,
) -> Option<AreaSoundReferences> {
    let [zone, area] = location.areas;
    let [root, group] = location.world_model_areas;
    let mut selected = None;
    let mut priority = 4;
    for row in rows {
        if location.world_model_only {
            if (root > 0 && root == row.world_model_area_id
                || group > 0 && group == row.world_model_area_id)
                && state_value(row.state[0]) == row.state[1]
            {
                selected = Some(row.sounds);
                if group == row.world_model_area_id {
                    return selected;
                }
            }
        } else if root == 0 && group == 0 {
            if (zone > 0 && zone == row.area_id || area > 0 && area == row.area_id)
                && state_value(row.state[0]) == row.state[1]
            {
                selected = Some(row.sounds);
                if area == row.area_id {
                    return selected;
                }
            }
        } else {
            let mut candidate = 4;
            if zone > 0 && zone == row.area_id {
                candidate = 3;
            }
            if area > 0 && area == row.area_id {
                candidate = 2;
            }
            // The mixed branch does not match root rows: this is the literal
            // 4CBF51..4CBF7E priority sequence, rather than inferred inheritance.
            if group > 0 && group == row.world_model_area_id {
                candidate = 0;
            }
            if candidate < priority && state_value(row.state[0]) == row.state[1] {
                selected = Some(row.sounds);
                priority = candidate;
                if priority == 0 {
                    return selected;
                }
            }
        }
    }
    selected
}

/// Selects 4C6810's masked chunk tuple with its original float stores.
/// Non-finite or unrepresentable coordinates cannot enter the native grid.
pub fn world_chunk_sound_key(
    map_id: u32,
    world_x: f32,
    world_y: f32,
) -> Option<WorldChunkSoundKey> {
    let origin = f64::from(17066.666_f32);
    let offsets = [
        origin - f64::from(world_y),
        f64::from((origin - f64::from(world_x)) as f32),
    ];
    let mut coordinates = [0i32; 2];
    for (coordinate, offset) in coordinates.iter_mut().zip(offsets) {
        let scaled = (offset * f64::from(0.03_f32)) as f32;
        let rounded = (f64::from(scaled) - 0.5).round_ties_even();
        if !rounded.is_finite() || rounded < f64::from(i32::MIN) || rounded > f64::from(i32::MAX) {
            return None;
        }
        *coordinate = rounded as i32;
    }
    Some(WorldChunkSoundKey {
        map_id,
        tile: coordinates.map(|coordinate| ((coordinate >> 4) & 63) as u32),
        chunk: coordinates.map(|coordinate| (coordinate & 15) as u32),
    })
}
