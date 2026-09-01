//! Owned pre-world object identity and startup diagnostics.

use crate::script::UiRuntimeObject;
use crate::{UiObjectKind, UiObjectRole};

/// Native screen selected immediately after built-in Glue has loaded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GlueInitialScreen {
    /// Startup login screen used after the intro request was consumed.
    Login,
    /// Full-screen startup movie selected by `playIntroMovie`.
    Movie,
}

impl GlueInitialScreen {
    pub(super) const fn script_name(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::Movie => "movie",
        }
    }
}

/// Durable identity and hierarchy for one instantiated GlueXML object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlueObject {
    name: Option<String>,
    kind: UiObjectKind,
    role: UiObjectRole,
    parent: Option<usize>,
    first_child: usize,
    child_count: usize,
}

impl GlueObject {
    /// Copies lifetime-independent state from one post-script Lua object.
    pub(super) fn from_runtime(
        object: &UiRuntimeObject,
        first_child: usize,
        child_count: usize,
    ) -> Self {
        Self {
            name: object.name.clone(),
            kind: object.kind,
            role: object.role,
            parent: object.parent,
            first_child,
            child_count,
        }
    }

    /// Returns the expanded global name when the object has one.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns the concrete stock object family.
    #[must_use]
    pub const fn kind(&self) -> UiObjectKind {
        self.kind
    }

    /// Returns the semantic widget slot occupied by this object.
    #[must_use]
    pub const fn role(&self) -> UiObjectRole {
        self.role
    }

    /// Returns the parent object arena index.
    #[must_use]
    pub const fn parent(&self) -> Option<usize> {
        self.parent
    }

    /// Returns the number of direct children in the manager's flat arena.
    #[must_use]
    pub const fn child_count(&self) -> usize {
        self.child_count
    }

    /// Returns the private flat-arena offset used by [`super::GlueManager`].
    pub(super) const fn first_child(&self) -> usize {
        self.first_child
    }
}

/// Immutable facts proving the built-in login UI completed startup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GlueStartupReport {
    resource_count: usize,
    action_count: usize,
    object_count: usize,
    named_object_count: usize,
    frame_count: usize,
    region_count: usize,
    texture_layer_count: usize,
    executed_chunk_count: usize,
    executed_load_handler_count: usize,
}

impl GlueStartupReport {
    /// Captures exact retained and executed startup counts.
    #[allow(clippy::too_many_arguments)]
    pub(super) const fn new(
        resource_count: usize,
        action_count: usize,
        object_count: usize,
        named_object_count: usize,
        frame_count: usize,
        region_count: usize,
        texture_layer_count: usize,
        executed_chunk_count: usize,
        executed_load_handler_count: usize,
    ) -> Self {
        Self {
            resource_count,
            action_count,
            object_count,
            named_object_count,
            frame_count,
            region_count,
            texture_layer_count,
            executed_chunk_count,
            executed_load_handler_count,
        }
    }

    /// Returns parsed XML and Lua resource count.
    #[must_use]
    pub const fn resource_count(self) -> usize {
        self.resource_count
    }

    /// Returns the fully expanded manifest action count.
    #[must_use]
    pub const fn action_count(self) -> usize {
        self.action_count
    }

    /// Returns instantiated live-object count.
    #[must_use]
    pub const fn object_count(self) -> usize {
        self.object_count
    }

    /// Returns objects published into the global Lua namespace.
    #[must_use]
    pub const fn named_object_count(self) -> usize {
        self.named_object_count
    }

    /// Returns resolved frame-derived object count.
    #[must_use]
    pub const fn frame_count(self) -> usize {
        self.frame_count
    }

    /// Returns resolved region state count.
    #[must_use]
    pub const fn region_count(self) -> usize {
        self.region_count
    }

    /// Returns retained property-bearing texture layers.
    #[must_use]
    pub const fn texture_layer_count(self) -> usize {
        self.texture_layer_count
    }

    /// Returns external and inline Lua chunks executed during startup.
    #[must_use]
    pub const fn executed_chunk_count(self) -> usize {
        self.executed_chunk_count
    }

    /// Returns XML `OnLoad` handlers executed during startup.
    #[must_use]
    pub const fn executed_load_handler_count(self) -> usize {
        self.executed_load_handler_count
    }
}
