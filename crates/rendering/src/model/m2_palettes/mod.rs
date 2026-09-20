//! Borrowed palette pages preserve worker ownership through synchronous GPU upload.

mod source;
mod upload;

pub use source::M2BonePaletteSource;
pub(crate) use upload::write_palette_bytes;

#[cfg(test)]
#[path = "../../../tests/model/m2_palettes.rs"]
mod tests;
