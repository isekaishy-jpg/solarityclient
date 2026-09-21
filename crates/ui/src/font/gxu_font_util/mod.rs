//! Shared font cache; production cache misses execute through the CPU host.

mod bitmap;
mod preparation;
mod state;
mod work;
pub use work::{FontGlyphRequest, FontWork, FontWorkExecutor, FontWorkOutput};

use crate::font::{FontError, RasterizedGlyph};
use solarity_asset::{AssetNamespaceId, AssetPath, AssetStore};
use state::{FontCache, FontSystemState};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
};
use work::{Reply, Request};

/// Stock font smoothing mode selected by a font object's `monochrome` flag.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FontRasterization {
    /// Grayscale antialiased coverage.
    Antialiased,
    /// One-bit coverage expanded to bytes for atlas storage.
    Monochrome,
}

#[derive(Clone)]
enum Owner {
    Local(Rc<RefCell<FontSystemState>>),
    Worker(Rc<WorkerFontSystem>),
}

struct WorkerFontSystem {
    cache: Arc<Mutex<FontCache>>,
    executor: Rc<dyn FontWorkExecutor>,
    preparations: RefCell<preparation::PreparationPool>,
}

impl WorkerFontSystem {
    fn cached<T>(
        &self,
        namespace: AssetNamespaceId,
        read: impl FnOnce(&FontCache) -> Option<T>,
    ) -> Result<Option<T>, FontError> {
        let cache = self.cache.lock().map_err(|_| work::unavailable())?;
        Ok((cache.provider == Some(namespace))
            .then(|| read(&cache))
            .flatten())
    }

    fn execute(&self, namespace: AssetNamespaceId, request: Request) -> Result<Reply, FontError> {
        self.executor
            .execute(FontWork {
                cache: Arc::clone(&self.cache),
                namespace,
                request,
            })
            .map(|result| result.0)
    }

    fn execute_prepared(
        &self,
        namespace: AssetNamespaceId,
        prepare: &mut dyn FnMut() -> Result<Request, FontError>,
    ) -> Result<Reply, FontError> {
        self.executor
            .execute_prepared(&mut || {
                Ok(FontWork {
                    cache: Arc::clone(&self.cache),
                    namespace,
                    request: prepare()?,
                })
            })
            .map(|result| result.0)
    }

    fn inspect<T>(&self, read: impl FnOnce(&FontCache) -> T) -> T {
        let cache = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        read(&cache)
    }
}

/// Clones share one namespace-aware glyph/metric authority. Production hosts keep
/// Lua-facing ownership on main and send only plain cache data and owned requests.
#[derive(Clone)]
pub struct FontSystem {
    owner: Owner,
}

impl std::fmt::Debug for FontSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontSystem")
            .field("faces", &self.loaded_face_count())
            .field("glyphs", &self.cached_glyph_count())
            .finish()
    }
}
impl PartialEq for FontSystem {
    fn eq(&self, other: &Self) -> bool {
        match (&self.owner, &other.owner) {
            (Owner::Local(left), Owner::Local(right)) => Rc::ptr_eq(left, right),
            (Owner::Worker(left), Owner::Worker(right)) => Rc::ptr_eq(left, right),
            _ => false,
        }
    }
}

impl FontSystem {
    /// Initializes a scoped native owner for worker-local preparation or offline use.
    /// # Errors
    /// Returns `FontError::Library` when FreeType initialization fails.
    pub fn new() -> Result<Self, FontError> {
        Ok(Self {
            owner: Owner::Local(Rc::new(RefCell::new(FontSystemState::new()?))),
        })
    }

    /// Uses the application's admitted worker service for all cache misses.
    /// No FreeType object is constructed on this calling thread.
    pub fn with_executor(executor: Rc<dyn FontWorkExecutor>) -> Self {
        Self {
            owner: Owner::Worker(Rc::new(WorkerFontSystem {
                cache: Arc::new(Mutex::new(FontCache::default())),
                executor,
                preparations: RefCell::default(),
            })),
        }
    }

    /// Returns exact archive-backed coverage, reusing the common warmed cache.
    /// # Errors
    /// Returns admission, native servicing or the exact archive/font failure.
    pub fn rasterize(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
        pixel_height: u32,
        character: char,
        rasterization: FontRasterization,
    ) -> Result<RasterizedGlyph, FontError> {
        match &self.owner {
            Owner::Local(state) => {
                state
                    .borrow_mut()
                    .rasterize(store, path, pixel_height, character, rasterization)
            }
            Owner::Worker(worker) => {
                if let Some(value) = worker.cached(store.namespace(), |cache| {
                    cache.glyph(path, pixel_height, character, rasterization)
                })? {
                    return Ok(value);
                }
                match worker.execute(
                    store.namespace(),
                    Request::Glyph(FontGlyphRequest::new(
                        path.clone(),
                        pixel_height,
                        character,
                        rasterization,
                    )),
                )? {
                    Reply::Glyph(value) => Ok(value),
                    _ => unreachable!("typed glyph reply"),
                }
            }
        }
    }

    /// Prepares an ordered atlas batch with one admitted miss operation.
    /// Individual glyph failures retain their original ordered consumer semantics.
    /// # Errors
    /// Returns host admission/completion failures; per-glyph errors remain in the output.
    pub fn rasterize_batch(
        &mut self,
        store: &mut AssetStore,
        requests: Vec<FontGlyphRequest>,
    ) -> Result<Vec<Result<RasterizedGlyph, FontError>>, FontError> {
        let result = match &self.owner {
            Owner::Local(state) => Request::Glyphs(requests).run(&mut state.borrow_mut(), store)?,
            Owner::Worker(worker) => {
                if requests.is_empty() {
                    return Ok(Vec::new());
                }
                if let Some(values) = worker.cached(store.namespace(), |cache| {
                    requests
                        .iter()
                        .map(|key| {
                            cache
                                .glyph(&key.face, key.height, key.character, key.mode)
                                .map(Ok)
                        })
                        .collect::<Option<Vec<_>>>()
                })? {
                    return Ok(values);
                }
                worker.execute(store.namespace(), Request::Glyphs(requests))?
            }
        };
        match result {
            Reply::Glyphs(values) => Ok(values),
            _ => unreachable!("typed glyph batch reply"),
        }
    }

    /// Measures stock unfitted whole-pixel advances without changing text semantics.
    /// # Errors
    /// Returns the same failures as character-advance preparation.
    pub fn measure_line_width_26_6(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
        pixel_height: u32,
        text: &str,
        rasterization: FontRasterization,
    ) -> Result<i64, FontError> {
        if let Owner::Local(state) = &self.owner {
            return state.borrow_mut().measure_line_width_26_6(
                store,
                path,
                pixel_height,
                text,
                rasterization,
            );
        }
        self.measure_character_advances_26_6(store, path, pixel_height, text, rasterization)
            .map(|values| values.iter().copied().fold(0_i64, i64::saturating_add))
    }

    pub(crate) fn measure_character_advances_26_6(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
        pixel_height: u32,
        text: &str,
        rasterization: FontRasterization,
    ) -> Result<crate::font::storage::FontBuffer<i64>, FontError> {
        match &self.owner {
            Owner::Local(state) => state.borrow_mut().measure_character_advances_26_6(
                store,
                path,
                pixel_height,
                text,
                rasterization,
            ),
            Owner::Worker(worker) => {
                if let Some(value) = worker.cached(store.namespace(), |cache| {
                    cache.advances(path, pixel_height, text, rasterization)
                })? {
                    match value {
                        Ok(value) => return Ok(value),
                        Err(FontError::Asset(solarity_asset::AssetError::SourceStorage(
                            solarity_cpu::CpuError::StorageAtCapacity { .. },
                        ))) => (),
                        Err(error) => return Err(error),
                    }
                }
                match worker.execute(
                    store.namespace(),
                    Request::Advances {
                        face: path.clone(),
                        height: pixel_height,
                        text: text.to_owned(),
                        mode: rasterization,
                    },
                )? {
                    Reply::Advances(value) => Ok(value),
                    _ => unreachable!("typed advances reply"),
                }
            }
        }
    }

    /// Returns the build-12340 face ascender at the exact requested pixel height.
    /// # Errors
    /// Returns host, archive, face or size failures without a substitute font.
    pub fn ascender_26_6(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
        pixel_height: u32,
    ) -> Result<i64, FontError> {
        match &self.owner {
            Owner::Local(state) => state.borrow_mut().ascender_26_6(store, path, pixel_height),
            Owner::Worker(worker) => {
                if let Some(value) = worker.cached(store.namespace(), |cache| {
                    cache.faces.get(path)?.ascenders.get(&pixel_height).copied()
                })? {
                    return Ok(value);
                }
                match worker.execute(
                    store.namespace(),
                    Request::Ascender {
                        face: path.clone(),
                        height: pixel_height,
                    },
                )? {
                    Reply::Metric(value) => Ok(value),
                    _ => unreachable!("typed ascender reply"),
                }
            }
        }
    }

    /// Returns stock hinted horizontal kerning for one adjacent character pair.
    /// # Errors
    /// Returns host, archive, face, size, glyph or FreeType kerning failures.
    pub fn kerning_x_26_6(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
        pixel_height: u32,
        left: char,
        right: char,
    ) -> Result<i64, FontError> {
        match &self.owner {
            Owner::Local(state) => {
                state
                    .borrow_mut()
                    .kerning_x_26_6(store, path, pixel_height, left, right)
            }
            Owner::Worker(worker) => {
                if let Some(value) = worker.cached(store.namespace(), |cache| {
                    cache
                        .faces
                        .get(path)?
                        .kerning
                        .get(&(pixel_height, left, right))
                        .copied()
                })? {
                    return Ok(value);
                }
                match worker.execute(
                    store.namespace(),
                    Request::Kerning {
                        face: path.clone(),
                        height: pixel_height,
                        left,
                        right,
                    },
                )? {
                    Reply::Metric(value) => Ok(value),
                    _ => unreachable!("typed kerning reply"),
                }
            }
        }
    }

    /// Reclaims cache-only coverage, scalar metrics and unused encoded faces.
    /// Atlas-held glyphs retain both their bytes and shared lookup identity.
    /// # Errors
    /// Returns worker admission or native servicing failures without serial cleanup.
    pub fn trim_unused(&self, assets: &mut AssetStore) -> Result<(), FontError> {
        match &self.owner {
            Owner::Local(state) => state.borrow_mut().trim_unused(),
            Owner::Worker(worker) => {
                match worker.execute_prepared(assets.namespace(), &mut || {
                    Ok(Request::Trim(std::mem::take(
                        &mut *worker.preparations.borrow_mut(),
                    )))
                })? {
                    Reply::Prepared => (),
                    _ => unreachable!("typed font trim reply"),
                }
            }
        }
        Ok(())
    }

    /// Returns distinct retained archive-backed faces.
    pub fn loaded_face_count(&self) -> usize {
        match &self.owner {
            Owner::Local(state) => state.borrow().loaded_face_count(),
            Owner::Worker(worker) => worker.inspect(|cache| cache.faces.len()),
        }
    }
    /// Returns retained face/size/mode/scalar coverage entries.
    pub fn cached_glyph_count(&self) -> usize {
        match &self.owner {
            Owner::Local(state) => state.borrow().glyph_count(),
            Owner::Worker(worker) => {
                worker.inspect(|cache| cache.faces.values().map(|face| face.glyphs.len()).sum())
            }
        }
    }
    /// Common coverage bytes, excluding atlas copies and shared references.
    pub fn cached_coverage_bytes(&self) -> usize {
        match &self.owner {
            Owner::Local(state) => state.borrow().coverage_bytes(),
            Owner::Worker(worker) => worker.inspect(|cache| {
                cache
                    .faces
                    .values()
                    .flat_map(|face| face.glyphs.values())
                    .map(|glyph| glyph.coverage().len())
                    .sum()
            }),
        }
    }
}
