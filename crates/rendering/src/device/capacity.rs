//! Shared high-water growth policy for rebuilt Vulkan frame resources.

/// Returns a geometric capacity that covers `required` without rebuilding for
/// every one-element increase in a dynamic stream.
///
/// Every allocation rounds the observed high-water mark to a power of two and
/// at least doubles existing storage. This prevents a loading burst that jumps
/// beyond `current * 2` from allocating its exact transient count and forcing
/// another device-idle rebuild when the following frame grows by one element.
pub(super) const fn geometric_capacity(current: usize, required: usize) -> usize {
    if current >= required {
        return current;
    }
    let rounded = match required.checked_next_power_of_two() {
        Some(rounded) => rounded,
        None => required,
    };
    let doubled = match current.checked_mul(2) {
        Some(doubled) => doubled,
        None => usize::MAX,
    };
    if rounded >= doubled { rounded } else { doubled }
}

#[cfg(test)]
mod tests {
    use super::geometric_capacity;

    #[test]
    fn first_frame_rounds_dynamic_capacity_up() {
        assert_eq!(geometric_capacity(0, 0), 0);
        assert_eq!(geometric_capacity(0, 1), 1);
        assert_eq!(geometric_capacity(0, 5), 8);
    }

    #[test]
    fn retained_capacity_doubles_or_covers_a_large_jump() {
        assert_eq!(geometric_capacity(8, 7), 8);
        assert_eq!(geometric_capacity(8, 9), 16);
        assert_eq!(geometric_capacity(8, 100), 128);
        assert_eq!(geometric_capacity(128, 296), 512);
        assert_eq!(geometric_capacity(1_024, 1_512), 2_048);
        assert_eq!(geometric_capacity(usize::MAX - 1, usize::MAX), usize::MAX);
    }
}
