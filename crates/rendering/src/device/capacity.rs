//! Shared high-water growth policy for rebuilt Vulkan frame resources.

/// Returns a geometric capacity that covers `required` without rebuilding for
/// every one-element increase in a dynamic stream.
///
/// The first allocation rounds to a power of two. Later allocations at least
/// double the previous high-water mark, while a large jump is admitted exactly
/// so overflow cannot manufacture an invalid capacity.
pub(super) const fn geometric_capacity(current: usize, required: usize) -> usize {
    if current >= required {
        return current;
    }
    if current == 0 {
        return match required.checked_next_power_of_two() {
            Some(rounded) => rounded,
            None => required,
        };
    }
    match current.checked_mul(2) {
        Some(doubled) if doubled > required => doubled,
        Some(_) | None => required,
    }
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
        assert_eq!(geometric_capacity(8, 100), 100);
        assert_eq!(geometric_capacity(usize::MAX - 1, usize::MAX), usize::MAX);
    }
}
