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

pub(super) trait Preparation: Send {
    fn run(self: Box<Self>, fonts: &mut super::FontSystem, assets: &mut AssetStore);
}

pub(super) enum Request {
    Trim(super::preparation::PreparationPool),
    Preparation(Box<dyn Preparation>),
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
    Prepared,
    Glyph(RasterizedGlyph),
    Glyphs(Vec<Result<RasterizedGlyph, FontError>>),
    Advances(crate::font::storage::FontBuffer<i64>),
    Metric(i64),
}

impl Request {
    pub(super) fn run(
        self,
        state: &mut FontSystemState,
        store: &mut AssetStore,
    ) -> Result<Reply, FontError> {
        match self {
            Self::Preparation(_) | Self::Trim(_) => {
                unreachable!("preparation and trim have dedicated owners")
            }
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

/// Main-only host admission and native waits for fonts and retained UI preparation.
/// Implementations must reclaim submitted work before returning or unwinding.
pub trait FontWorkExecutor {
    /// Executes the request without running Lua or publishing UI state on a worker.
    /// # Errors
    /// Returns admission, completion, native servicing or exact font failures.
    fn execute(&self, work: FontWork) -> Result<FontWorkOutput, FontError>;

    /// Builds owned inputs only after task admission. Scheduled hosts override this
    /// boundary; synchronous/offline hosts can execute the factory directly.
    /// The factory is called at most once and all submitted work is reclaimed before return.
    /// # Errors
    /// Returns admission, preparation or execution failure without losing borrowed state.
    fn execute_prepared(
        &self,
        prepare: &mut dyn FnMut() -> Result<FontWork, FontError>,
    ) -> Result<FontWorkOutput, FontError> {
        self.execute(prepare()?)
    }

    /// The stable application budget for retained handoff storage and owned captures.
    /// Offline hosts may omit accounting along with executor ownership.
    fn storage(&self) -> Option<&solarity_cpu::CpuStorageBudget> {
        None
    }
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
        if let Request::Trim(loans) = self.request {
            cache.trim_unused();
            drop(cache);
            // Typed placeholder destructors execute after font metadata is unlocked.
            drop(loans);
            return Ok(FontWorkOutput(Reply::Prepared));
        }
        let mut state = FontSystemState::new()?;
        state.cache = std::mem::take(&mut *cache);
        if let Request::Preparation(operation) = self.request {
            let local = std::rc::Rc::new(std::cell::RefCell::new(state));
            let mut fonts = super::FontSystem {
                owner: super::Owner::Local(local.clone()),
            };
            operation.run(&mut fonts, store);
            *cache = std::mem::take(&mut local.borrow_mut().cache);
            return Ok(FontWorkOutput(Reply::Prepared));
        }
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
    ) -> Option<Result<crate::font::storage::FontBuffer<i64>, FontError>> {
        let cached = self.faces.get(face)?;
        let mut values = crate::font::storage::FontBuffer::default();
        if let Err(error) = values.reserve(cached.advances.policy(), text.chars().count()) {
            return Some(Err(error.into()));
        }
        for character in text.chars() {
            let advance = *cached.advances.get(&(height, mode, character))?;
            if let Err(error) = values.push(cached.advances.policy(), advance) {
                return Some(Err(error.into()));
            }
        }
        Some(Ok(values))
    }
}
