//! Client presentation changes driven by authoritative combat outcomes.

/// Applies the signed health delta in native 71C260 to the client's prediction.
/// The replicated health field remains owned by object updates.
#[must_use]
pub const fn predict_unit_health(
    current: i32,
    maximum: i32,
    secondary_flags: u32,
    delta: i32,
) -> i32 {
    if delta < 0 && secondary_flags & 0x100 != 0 {
        return current;
    }
    let health = current.wrapping_add(delta);
    if health < 1 {
        1
    } else if maximum < health {
        maximum
    } else {
        health
    }
}
