//! Owned font requests share plain cache data, never native FreeType owners.

use super::{
    FontRasterization,
    state::{FontCache, FontSystemState},
};
use crate::{FontError, RasterizedGlyph};
use solarity_asset::{AssetNamespaceId, AssetPath, AssetStore};
use std::sync::{Arc, Mutex};

/// One exact face/size/mode/scalar request in an ordered atlas batch.
#[derive(Clone)]
pub struct FontGlyphRequest {
    pub(super) face: AssetPath,
    pub(super) height: u32,
    pub(super) character: char,
    pub(super) mode: FontRasterization,
}

impl FontGlyphRequest {
    /// Captures the same identity used by the common coverage cache.
    pub fn new(face: AssetPath, height: u32, character: char, mode: FontRasterization) -> Self {
        Self {
            face,
            height,
            character,
            mode,
        }
    }
}

pub(super) enum Request {
    Glyph(FontGlyphRequest),
    Glyphs(Vec<FontGlyphRequest>),
    Advances {
        face: AssetPath,
        height: u32,
        text: String,
        mode: FontRasterization,
    },
    Ascender {
        face: AssetPath,
        height: u32,
    },
    Kerning {
        face: AssetPath,
        height: u32,
        left: char,
        right: char,
    },
}

pub(super) enum Reply {
    Glyph(RasterizedGlyph),
    Glyphs(Vec<Result<RasterizedGlyph, FontError>>),
    Advances(Vec<i64>),
    Metric(i64),
}

impl Request {
    pub(super) fn run(
        self,
        state: &mut FontSystemState,
        store: &mut AssetStore,
    ) -> Result<Reply, FontError> {
        match self {
            Self::Glyph(key) => state
                .rasterize(store, &key.face, key.height, key.character, key.mode)
                .map(Reply::Glyph),
            Self::Glyphs(keys) => Ok(Reply::Glyphs(
                keys.into_iter()
                    .map(|key| {
                        state.rasterize(store, &key.face, key.height, key.character, key.mode)
                    })
                    .collect(),
            )),
            Self::Advances {
                face,
                height,
                text,
                mode,
            } => state
                .measure_character_advances_26_6(store, &face, height, &text, mode)
                .map(Reply::Advances),
            Self::Ascender { face, height } => {
                state.ascender_26_6(store, &face, height).map(Reply::Metric)
            }
            Self::Kerning {
                face,
                height,
                left,
                right,
            } => state
                .kerning_x_26_6(store, &face, height, left, right)
                .map(Reply::Metric),
        }
    }
}

/// Main-only host admission and native-wait policy for an owned font request.
/// Implementations submit to the application CPU service and reclaim before returning.
pub trait FontWorkExecutor {
    /// Executes the request without running Lua or publishing UI state on a worker.
    /// # Errors
    /// Returns admission, completion, native servicing or exact font failures.
    fn execute(&self, work: FontWork) -> Result<FontWorkOutput, FontError>;
}

/// Sendable request and a pin on its sole common coverage/metric authority.
pub struct FontWork {
    pub(super) cache: Arc<Mutex<FontCache>>,
    pub(super) namespace: AssetNamespaceId,
    pub(super) request: Request,
}

/// Typed private reply; only the matching FontSystem consumes its payload.
pub struct FontWorkOutput(pub(super) Reply);

impl FontWork {
    /// Runs on the admitted worker with its archive reader and read-byte policy.
    /// Native faces and their library are retired before any reply is published.
    /// # Errors
    /// Rejects a changed source namespace, cache poisoning, or the exact font failure.
    pub fn run(self, store: &mut AssetStore) -> Result<FontWorkOutput, FontError> {
        if self.namespace != store.namespace() {
            return Err(FontError::Execution {
                message: "font worker archive namespace changed".into(),
            });
        }
        let mut cache = self.cache.lock().map_err(|_| unavailable())?;
        let mut state = FontSystemState::new()?;
        state.cache = std::mem::take(&mut *cache);
        let result = self.request.run(&mut state, store);
        *cache = state.cache;
        result.map(FontWorkOutput)
    }
}

pub(super) fn unavailable() -> FontError {
    FontError::Execution {
        message: "font preparation cache is unavailable".into(),
    }
}

impl FontCache {
    pub(super) fn glyph(
        &self,
        face: &AssetPath,
        height: u32,
        character: char,
        mode: FontRasterization,
    ) -> Option<RasterizedGlyph> {
        self.faces
            .get(face)?
            .glyphs
            .get(&(height, mode, character))
            .cloned()
    }

    pub(super) fn advances(
        &self,
        face: &AssetPath,
        height: u32,
        text: &str,
        mode: FontRasterization,
    ) -> Option<Vec<i64>> {
        let cached = self.faces.get(face)?;
        text.chars()
            .map(|character| cached.advances.get(&(height, mode, character)).copied())
            .collect()
    }
}
