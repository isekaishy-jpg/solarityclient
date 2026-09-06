//! Per-widget state retained separately from the shared minimap scene.

use solarity_asset::AssetPath;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MinimapWidgetState {
    pub(crate) player_texture: AssetPath,
    pub(crate) player_size: [f32; 2],
    pub(crate) ping: Option<[f64; 2]>,
}
