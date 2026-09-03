//! Stable failures while preparing renderer-owned UI mesh data.

use thiserror::Error;

/// A resolved UI quad cannot be represented by the fixed Vulkan mesh ABI.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum UiMeshPlanError {
    /// The logical canvas is absent or cannot be represented as finite floats.
    #[error("invalid UI render extent {width}x{height}")]
    InvalidExtent {
        /// Logical canvas width.
        width: f32,
        /// Logical canvas height.
        height: f32,
    },
    /// A quad contains a coordinate or color that is not finite.
    #[error("UI object {object_index} has non-finite {field} component {component}")]
    NonFiniteComponent {
        /// Live UI object-arena identity.
        object_index: usize,
        /// Vertex field containing the invalid component.
        field: &'static str,
        /// Zero-based component within the flattened field.
        component: usize,
    },
    /// A quad's right or upper edge precedes its opposite edge.
    #[error("UI object {object_index} has inverted screen bounds")]
    InvertedBounds {
        /// Live UI object-arena identity.
        object_index: usize,
    },
    /// Vertex or index offsets exceed the renderer's unsigned 32-bit draw ABI.
    #[error("UI mesh exceeds the unsigned 32-bit {domain} capacity")]
    Capacity {
        /// Mesh array whose offset cannot be represented.
        domain: &'static str,
    },
    /// An indexed tooling mesh refers beyond its supplied vertex array.
    #[error("UI mesh index {index} exceeds the available {vertex_count} vertices")]
    IndexOutOfRange {
        /// Invalid vertex index.
        index: u32,
        /// Number of available vertices.
        vertex_count: u32,
    },
}
