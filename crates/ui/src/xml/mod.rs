//! FrameXML and GlueXML parsing, template inheritance, and object construction.
//!
//! `XMLTree.cpp` and the `CSimple*` object families establish an XML-driven UI
//! pipeline. `quick-xml` provides tokenization; this module owns stock schema,
//! load order, inheritance, validation, and Lua binding behavior.

mod xml_tree;
