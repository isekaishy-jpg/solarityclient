//! Player-capability tracking list consumed by the minimap dropdown.

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Lua, MultiValue, Table, Value};
use thiserror::Error;

/// Origin used by FrameXML to select icon coordinate treatment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiTrackingCategory {
    /// Spellbook capability such as Find Minerals or Track Humanoids.
    Spell,
    /// Client-provided area or service tracking capability.
    Area,
}

impl UiTrackingCategory {
    /// Returns the exact FrameXML category token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Spell => "spell",
            Self::Area => "area",
        }
    }
}

/// One ordered tracking capability visible to the local player.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiTrackingType {
    name: String,
    texture: String,
    category: UiTrackingCategory,
}

impl UiTrackingType {
    /// Creates one complete tracking row.
    ///
    /// # Errors
    ///
    /// Returns [`UiTrackingError`] for an empty label or texture identity.
    pub fn new(
        name: impl Into<String>,
        texture: impl Into<String>,
        category: UiTrackingCategory,
    ) -> Result<Self, UiTrackingError> {
        let name = name.into();
        let texture = texture.into();
        if name.is_empty() {
            return Err(UiTrackingError::EmptyName);
        }
        if texture.is_empty() {
            return Err(UiTrackingError::EmptyTexture);
        }
        Ok(Self {
            name,
            texture,
            category,
        })
    }

    /// Returns the localized capability label.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the icon texture identity.
    #[must_use]
    pub fn texture(&self) -> &str {
        &self.texture
    }

    /// Returns the icon-coordinate category.
    #[must_use]
    pub const fn category(&self) -> UiTrackingCategory {
        self.category
    }
}

#[derive(Debug, Default)]
struct UiMinimapTrackingInner {
    types: Vec<UiTrackingType>,
    active: Option<usize>,
}

/// Shared ordered player tracking capabilities and exclusive selection.
#[derive(Clone, Debug, Default)]
pub struct UiMinimapTrackingState {
    inner: Rc<RefCell<UiMinimapTrackingInner>>,
}

impl UiMinimapTrackingState {
    /// Creates the pre-capability state with no synthesized rows.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the ordered list after player spells or area capabilities change.
    pub fn replace_types(&self, types: Vec<UiTrackingType>) {
        let mut inner = self.inner.borrow_mut();
        inner.types = types;
        inner.active = None;
    }

    /// Returns the number of script-visible tracking rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.borrow().types.len()
    }

    /// Reports whether no tracking rows are available.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.borrow().types.is_empty()
    }

    /// Returns one one-based row and its selection state.
    ///
    /// # Errors
    ///
    /// Returns [`UiTrackingError`] when `index` is unavailable.
    pub fn info(&self, index: usize) -> Result<(UiTrackingType, bool), UiTrackingError> {
        let inner = self.inner.borrow();
        let internal = index
            .checked_sub(1)
            .ok_or(UiTrackingError::InvalidIndex { index })?;
        let tracking = inner
            .types
            .get(internal)
            .cloned()
            .ok_or(UiTrackingError::InvalidIndex { index })?;
        Ok((tracking, inner.active == Some(internal)))
    }

    /// Selects one one-based row, or clears tracking for `None`.
    ///
    /// # Errors
    ///
    /// Returns [`UiTrackingError`] when an index is unavailable.
    pub fn select(&self, index: Option<usize>) -> Result<(), UiTrackingError> {
        let mut inner = self.inner.borrow_mut();
        inner.active = match index {
            None => None,
            Some(index) => {
                let internal = index
                    .checked_sub(1)
                    .ok_or(UiTrackingError::InvalidIndex { index })?;
                if internal >= inner.types.len() {
                    return Err(UiTrackingError::InvalidIndex { index });
                }
                Some(internal)
            }
        };
        Ok(())
    }

    /// Returns the selected row's texture identity.
    #[must_use]
    pub fn active_texture(&self) -> Option<String> {
        let inner = self.inner.borrow();
        inner
            .active
            .and_then(|index| inner.types.get(index))
            .map(|tracking| tracking.texture.clone())
    }
}

/// Invalid minimap tracking state or script index.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UiTrackingError {
    /// A capability lacks its localized label.
    #[error("minimap tracking capability has an empty name")]
    EmptyName,
    /// A capability lacks its icon texture identity.
    #[error("minimap tracking capability has an empty texture")]
    EmptyTexture,
    /// A caller addressed an unavailable one-based row.
    #[error("minimap tracking index {index} is unavailable")]
    InvalidIndex {
        /// Rejected one-based index.
        index: usize,
    },
}

/// Registers the build-12340 minimap tracking global family.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiMinimapTrackingState,
) -> mlua::Result<()> {
    let info_state = state.clone();
    let selection_state = state.clone();
    let texture_state = state.clone();
    globals.raw_set(
        "GetNumTrackingTypes",
        lua.create_function(move |_, ()| Ok(state.len()))?,
    )?;
    globals.raw_set(
        "GetTrackingInfo",
        lua.create_function(move |lua, index: usize| {
            let (tracking, active) = info_state
                .info(index)
                .map_err(|error| mlua::Error::runtime(error.to_string()))?;
            let mut values = MultiValue::new();
            values.push_back(Value::String(lua.create_string(tracking.name())?));
            values.push_back(Value::String(lua.create_string(tracking.texture())?));
            values.push_back(Value::Boolean(active));
            values.push_back(Value::String(
                lua.create_string(tracking.category().as_str())?,
            ));
            Ok(values)
        })?,
    )?;
    globals.raw_set(
        "SetTracking",
        lua.create_function(move |_, index: Option<usize>| {
            selection_state
                .select(index)
                .map_err(|error| mlua::Error::runtime(error.to_string()))
        })?,
    )?;
    globals.raw_set(
        "GetTrackingTexture",
        lua.create_function(move |_, ()| Ok(texture_state.active_texture()))?,
    )
}
