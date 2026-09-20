//! Ordered model palette pages without a concatenated CPU frame allocation.

use glam::Mat4;

/// Immutable palette pages in the same order used to relocate draw bone indices.
///
/// The sum of all page lengths must equal `len()`. Pages and length remain stable
/// for the borrow; rendering copies them to a fence-owned GPU slot before return.
/// Empty pages are allowed so retained job banks need no second index allocation.
pub trait M2BonePaletteSource {
    /// Total number of transforms in the logical concatenated palette.
    fn len(&self) -> usize;

    /// Whether the logical palette contains no transforms.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Number of borrowed pages, including empty pages.
    fn palette_count(&self) -> usize;

    /// Borrows one page. The index must be smaller than `palette_count()`.
    fn palette(&self, index: usize) -> &[Mat4];
}

impl M2BonePaletteSource for [Mat4] {
    fn len(&self) -> usize {
        self.len()
    }
    fn palette_count(&self) -> usize {
        1
    }
    fn palette(&self, index: usize) -> &[Mat4] {
        [self][index]
    }
}

impl M2BonePaletteSource for Vec<Mat4> {
    fn len(&self) -> usize {
        self.len()
    }
    fn palette_count(&self) -> usize {
        1
    }
    fn palette(&self, index: usize) -> &[Mat4] {
        [self.as_slice()][index]
    }
}

impl<const N: usize> M2BonePaletteSource for [Mat4; N] {
    fn len(&self) -> usize {
        N
    }
    fn palette_count(&self) -> usize {
        1
    }
    fn palette(&self, index: usize) -> &[Mat4] {
        [self.as_slice()][index]
    }
}
