//! Final screen-pixel alignment of an ordinary GxuFontString origin.

/// The justification anchor shared by all glyph passes of one live string.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TextOrigin {
    anchor: [f64; 2],
    pixels_per_ui_unit: f64,
}

impl TextOrigin {
    /// Retains the local justification anchor after line layout has completed.
    pub(super) const fn new(anchor: [f64; 2], pixels_per_ui_unit: f64) -> Self {
        Self {
            anchor,
            pixels_per_ui_unit,
        }
    }

    /// Applies the native origin floor and font projection before glyph bearings.
    pub(super) fn offset(self, owner: [f64; 2], scale: f64) -> [f64; 2] {
        std::array::from_fn(|axis| {
            let anchor = owner[axis] + self.anchor[axis] * scale;
            // 006C0BA0 projects the floored 006C6190 origin half a pixel right
            // and down on D3D9's integer sample grid. Vulkan's half-integer
            // grid needs another half pixel to sample the same glyph texels.
            // This is one physical pixel, independent of UI or owner scale.
            let projection = if axis == 0 { 1.0 } else { -1.0 };
            ((anchor * self.pixels_per_ui_unit).floor() + projection) / self.pixels_per_ui_unit
                - anchor
        })
    }
}

#[cfg(test)]
#[path = "../../tests/font/text_origin.rs"]
mod tests;
