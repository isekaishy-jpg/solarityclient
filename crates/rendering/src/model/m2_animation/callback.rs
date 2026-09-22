//! The shared active-bone scan in `CM2Model::UpdateSequences` (832260).

use solarity_asset::M2AnimationSet;

use super::M2ModelSequenceTimer;

/// One active bone timer in native linked-list order (new activations first).
#[derive(Clone, Copy, Debug)]
pub struct M2CallbackSlot {
    /// Actual bone index, with zero owning the model's primary sequence.
    pub bone: u16,
    /// Resolved sequence record selected on this bone.
    pub sequence: usize,
    /// Current primary timer of this bone slot.
    pub timer: M2ModelSequenceTimer,
    /// Suppresses both authored events and further completion callbacks.
    pub finished: bool,
}

/// A queued callback retains its originating timer across user callbacks.
#[derive(Clone, Copy, Debug)]
pub struct M2QueuedCallback {
    /// Sequence and start-time snapshot checked at completion dispatch.
    pub slot: M2CallbackSlot,
    /// `None` is a sequence completion; otherwise the authored event index.
    pub event: Option<usize>,
    /// Callback boundary, which can precede the actual current scene tick.
    pub scene_time_ms: u32,
}

/// Reuses the queue for one native scan. Dispatch it before scanning again
/// with its returned cursor, since callbacks can change either bone timer.
///
/// Completion eligibility is decided *before* scanning that slot's events.
/// Thus its completion can replace an earlier event queued by the same slot.
/// Sorting every candidate by time would change native behavior.
pub fn scan_m2_callbacks(
    animations: &M2AnimationSet,
    slots: impl IntoIterator<Item = M2CallbackSlot>,
    previous_ms: u32,
    current_ms: u32,
    events_enabled: bool,
    queue: &mut Vec<M2QueuedCallback>,
) -> u32 {
    scan_m2_callbacks_into(
        animations,
        slots,
        previous_ms,
        current_ms,
        events_enabled,
        queue,
    )
    .unwrap_or_else(|error| panic!("callback queue allocation failed: {error}"))
}

/// Authored upper bound for one scan, including duplicate/collapsed event ticks.
/// Every timestamp can contribute once at the selected tick, plus one completion
/// per slot. Taking each event's largest channel also covers sequence changes.
/// # Errors
/// Reports size overflow before allocating any queue storage.
pub fn m2_callback_queue_capacity(
    animations: &M2AnimationSet,
    slots: usize,
) -> Result<usize, solarity_cpu::CpuError> {
    let per_slot = animations
        .events()
        .iter()
        .try_fold(1_usize, |count, event| {
            count
                .checked_add(
                    event
                        .timeline()
                        .channels()
                        .iter()
                        .map(Vec::len)
                        .max()
                        .unwrap_or(0),
                )
                .ok_or(solarity_cpu::CpuError::StorageSizeOverflow)
        })?;
    slots
        .checked_mul(per_slot)
        .ok_or(solarity_cpu::CpuError::StorageSizeOverflow)
}

/// Preserves native scan order while appending through an admitted writer.
/// # Errors
/// Returns capacity/allocation failure without dispatching a partial callback queue.
pub fn scan_m2_callbacks_into(
    animations: &M2AnimationSet,
    slots: impl IntoIterator<Item = M2CallbackSlot>,
    previous_ms: u32,
    current_ms: u32,
    events_enabled: bool,
    queue: &mut impl solarity_cpu::OutputBuffer<M2QueuedCallback>,
) -> Result<u32, solarity_cpu::CpuError> {
    queue.clear();
    let mut nearest = current_ms;
    if current_ms.wrapping_sub(previous_ms) as i32 <= 0 {
        return Ok(nearest);
    }
    for slot in slots {
        if slot.finished || usize::from(slot.bone) >= animations.bones().len() {
            continue;
        }
        // An overdue terminal deadline can move nearest behind the shared
        // cursor. Later slots still compare their deadlines with that bound.
        let completion = slot
            .timer
            .next_completion_ms(previous_ms, current_ms)
            .filter(|tick| nearest.wrapping_sub(*tick) as i32 >= 0);
        if events_enabled {
            for (index, event) in animations.events().iter().enumerate() {
                if !event_reaches_slot(animations, event.bone_index(), slot.bone) {
                    continue;
                }
                let channel = if event.timeline().global_sequence().is_some() {
                    0
                } else {
                    slot.sequence
                };
                let Some(timestamps) = event.timeline().channels().get(channel) else {
                    continue;
                };
                let mut error = None;
                slot.timer
                    .visit_event_ticks(timestamps, previous_ms, nearest, |tick| {
                        if error.is_none() && nearest.wrapping_sub(tick) as i32 >= 0 {
                            if tick != nearest {
                                queue.clear();
                                nearest = tick;
                            }
                            error = queue
                                .try_reserve_exact(1)
                                .and_then(|()| {
                                    queue.push(M2QueuedCallback {
                                        slot,
                                        event: Some(index),
                                        scene_time_ms: tick,
                                    })
                                })
                                .err();
                        }
                    });
                if let Some(error) = error {
                    queue.clear();
                    return Err(error);
                }
            }
        }
        if let Some(tick) = completion {
            // 82E790 omits anonymous non-root bones before touching the queue.
            if slot.bone != 0
                && animations
                    .bones()
                    .get(usize::from(slot.bone))
                    .is_none_or(|bone| bone.key_bone_id() == -1)
            {
                continue;
            }
            if tick != nearest {
                queue.clear();
                nearest = tick;
            }
            if let Err(error) = queue.try_reserve_exact(1).and_then(|()| {
                queue.push(M2QueuedCallback {
                    slot,
                    event: None,
                    scene_time_ms: tick,
                })
            }) {
                queue.clear();
                return Err(error);
            }
        }
    }
    Ok(nearest)
}

/// 830FB0 starts ancestry traversal at the event bone's parent. A parentless
/// event bypasses the traversal, so it can occur for multiple active slots.
fn event_reaches_slot(animations: &M2AnimationSet, event_bone: Option<u32>, slot: u16) -> bool {
    if slot == 0 {
        return true;
    }
    let Some(bone) = event_bone.and_then(|index| animations.bones().get(index as usize)) else {
        return false;
    };
    let Some(mut parent) = bone.parent() else {
        return true;
    };
    loop {
        if parent == slot {
            return true;
        }
        let Some(next) = animations
            .bones()
            .get(usize::from(parent))
            .and_then(|bone| bone.parent())
        else {
            return false;
        };
        parent = next;
    }
}
