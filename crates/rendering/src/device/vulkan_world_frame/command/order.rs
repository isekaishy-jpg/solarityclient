//! The single stock scene order is captured before parallel command recording.

use super::draws::{
    effect_scene, m2_scene_set, record_liquid_queue, record_m2, record_particle, record_ribbon,
};
use super::instances;
use super::sky::record_ripples;
use super::{RecordContext, WorldCommandBindings};
use crate::VulkanError;

/// Native liquid material flags select the two preserved world queue positions.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum LiquidQueue {
    Opaque,
    Transparent,
}
/// Dispatches the typed streams in their one stock scene-element order.
pub(super) fn record_m2_scene_elements(
    context: &RecordContext<'_>,
    bindings: &mut WorldCommandBindings,
) -> Result<(), VulkanError> {
    let mut next_m2 = 0;
    let mut next_particle = 0;
    let mut next_ribbon = 0;
    let mut water_pending = !context.liquid_draws.is_empty()
        || context
            .ripple_frame
            .is_some_and(|frame| frame.draw_count() != 0);
    loop {
        let m2_key = context
            .m2_draws
            .get(next_m2)
            .map(|draw| (draw.scene_order(), 0_u8));
        let particle_key = context
            .particle_draws
            .get(next_particle)
            .map(|draw| (draw.scene_order(), 4_u8));
        let ribbon_key = context
            .ribbon_draws
            .get(next_ribbon)
            .map(|draw| (draw.scene_order(), 3_u8));
        let next = [
            m2_key.map(|key| (key, 0_u8)),
            particle_key.map(|key| (key, 1_u8)),
            ribbon_key.map(|key| (key, 2_u8)),
        ]
        .into_iter()
        .flatten()
        .min_by_key(|(key, _kind)| *key);
        if water_pending
            && next.is_none_or(|((order, _kind), _)| order >= context.liquid_scene_order)
        {
            record_liquid_queue(context, LiquidQueue::Transparent, bindings)?;
            record_ripples(context, bindings)?;
            water_pending = false;
        }
        match next.map(|(_key, kind)| kind) {
            Some(0) => {
                let draw = context
                    .m2_draws
                    .get(next_m2)
                    .copied()
                    .ok_or(VulkanError::WorldFrameCapacity)?;
                let boundary = particle_key
                    .into_iter()
                    .chain(ribbon_key)
                    .map(|(order, _)| order)
                    .chain(water_pending.then_some(context.liquid_scene_order))
                    .min()
                    .unwrap_or(u32::MAX);
                let count = 1 + context.m2_draws[next_m2 + 1..]
                    .iter()
                    .take_while(|candidate| {
                        candidate.scene_order() < boundary
                            && instances::compatible(context, draw, **candidate)
                    })
                    .count();
                record_m2(
                    context,
                    next_m2,
                    draw,
                    u32::try_from(count).map_err(|_| VulkanError::WorldFrameCapacity)?,
                    m2_scene_set(context, draw.light_bank()),
                    effect_scene(context, draw.scene_index(), draw.light_bank())?,
                    bindings,
                )?;
                next_m2 += count;
            }
            Some(1) => {
                let draw = context
                    .particle_draws
                    .get(next_particle)
                    .copied()
                    .ok_or(VulkanError::WorldFrameCapacity)?;
                record_particle(context, draw, bindings)?;
                next_particle += 1;
            }
            Some(2) => {
                let draw = context
                    .ribbon_draws
                    .get(next_ribbon)
                    .copied()
                    .ok_or(VulkanError::WorldFrameCapacity)?;
                record_ribbon(context, draw, bindings)?;
                next_ribbon += 1;
            }
            Some(_) => return Err(VulkanError::WorldFrameCapacity),
            None => return Ok(()),
        }
    }
}

/// Each native sky model scene completes before the next sky compositor layer.
pub(super) fn record_sky_models(
    context: &RecordContext<'_>,
    skyboxes: bool,
    bindings: &mut WorldCommandBindings,
) -> Result<(), VulkanError> {
    let Some(frame) = context.sky_models else {
        return Ok(());
    };
    let mut index = context.m2_draws.len();
    if skyboxes {
        index += frame.stars.len();
        for (slot, batch) in frame.skyboxes.iter().enumerate() {
            for draw in batch.draws {
                record_m2(
                    context,
                    index,
                    *draw,
                    1,
                    context.frame_sets[9 + slot],
                    batch.scene,
                    bindings,
                )?;
                index += 1;
            }
        }
    } else {
        for draw in frame.stars {
            record_m2(
                context,
                index,
                *draw,
                1,
                context.frame_sets[8],
                frame.scene,
                bindings,
            )?;
            index += 1;
        }
    }
    Ok(())
}
