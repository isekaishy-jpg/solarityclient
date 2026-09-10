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

    /// Applies 006C6190's final floor in physical pixels, before glyph bearings.
    pub(super) fn offset(self, owner: [f64; 2], scale: f64) -> [f64; 2] {
        std::array::from_fn(|axis| {
            let anchor = owner[axis] + self.anchor[axis] * scale;
            (anchor * self.pixels_per_ui_unit).floor() / self.pixels_per_ui_unit - anchor
        })
    }
}

#[cfg(test)]
#[path = "../../tests/font/text_origin.rs"]
mod tests;
