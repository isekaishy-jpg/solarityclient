//! Nested live object construction and global name ownership.

use std::collections::HashMap;
use std::ops::Range;

use solarity_asset::AssetPath;

use crate::frame::c_simple_frame::inherited_object_kind;
use crate::frame::{UiObjectCatalog, UiObjectDefinition, UiObjectError, UiObjectKind};
use crate::{FontCatalog, XmlContent, XmlDocument, XmlElement};

/// Stock region draw bands in back-to-front order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum UiDrawLayer {
    /// Frame background artwork.
    Background,
    /// Frame border artwork.
    Border,
    /// Ordinary frame artwork and the stock XML default.
    Artwork,
    /// Foreground artwork and text.
    Overlay,
    /// Interaction highlight artwork.
    Highlight,
}

/// The semantic slot occupied by a child object in its owning widget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiObjectRole {
    /// An ordinary frame, region, or widget declaration.
    Object,
    /// Button label font string.
    ButtonText,
    /// Default button texture.
    NormalTexture,
    /// Pressed button texture.
    PushedTexture,
    /// Disabled button texture.
    DisabledTexture,
    /// Mouse-highlight texture.
    HighlightTexture,
    /// Checked check-button texture.
    CheckedTexture,
    /// Disabled checked texture.
    DisabledCheckedTexture,
    /// Slider thumb texture.
    ThumbTexture,
}

/// One live-root construction batch aligned to an expanded bundle action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiObjectBatch {
    action_index: usize,
    root: usize,
    first_node: usize,
    node_count: usize,
}

impl UiObjectBatch {
    /// Returns the bundle action that constructs this live root.
    #[must_use]
    pub const fn action_index(self) -> usize {
        self.action_index
    }

    /// Returns the root object arena index for this batch.
    #[must_use]
    pub const fn root(self) -> usize {
        self.root
    }

    /// Returns newly allocated objects in exact construction order.
    #[must_use]
    pub fn node_range(self) -> Range<usize> {
        self.first_node..self.first_node + self.node_count
    }

    /// Returns the number of newly allocated objects in this batch.
    #[must_use]
    pub const fn node_count(self) -> usize {
        self.node_count
    }
}

/// One XML layer contributing properties and nested objects to an instance.
#[derive(Clone, Copy)]
pub struct UiElementLayer<'bundle> {
    source_path: &'bundle AssetPath,
    document: &'bundle XmlDocument,
    element: &'bundle XmlElement,
    draw_layer: Option<UiDrawLayer>,
}

impl<'bundle> UiElementLayer<'bundle> {
    /// Returns the XML asset containing this layer.
    #[must_use]
    pub const fn source_path(&self) -> &AssetPath {
        self.source_path
    }

    /// Returns the owning XML document.
    #[must_use]
    pub const fn document(&self) -> &XmlDocument {
        self.document
    }

    /// Returns the contributing XML element.
    #[must_use]
    pub const fn element(&self) -> &XmlElement {
        self.element
    }

    /// Returns the enclosing stock draw band for a layered region declaration.
    #[must_use]
    pub const fn draw_layer(&self) -> Option<UiDrawLayer> {
        self.draw_layer
    }

    fn same_source(self, other: Self) -> bool {
        self.source_path == other.source_path
            && std::ptr::eq(self.document, other.document)
            && std::ptr::eq(self.element, other.element)
    }
}

/// One instantiated frame, region, or widget in the complete UI object arena.
pub struct UiObjectNode<'bundle> {
    name: Option<String>,
    name_context: Option<String>,
    kind: UiObjectKind,
    role: UiObjectRole,
    parent: Option<usize>,
    children: Vec<usize>,
    construction_parent: Option<usize>,
    construction_children: Vec<usize>,
    layers: Vec<UiElementLayer<'bundle>>,
}

impl<'bundle> UiObjectNode<'bundle> {
    /// Returns the expanded global name when this object has one.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns the nearest expanded name used for descendant `$parent` values.
    #[must_use]
    pub fn name_context(&self) -> Option<&str> {
        self.name_context.as_deref()
    }

    /// Returns the concrete stock object type.
    #[must_use]
    pub const fn kind(&self) -> UiObjectKind {
        self.kind
    }

    /// Returns the widget slot occupied by this object.
    #[must_use]
    pub const fn role(&self) -> UiObjectRole {
        self.role
    }

    /// Returns the final XML `parentKey` used to publish this child on its
    /// construction owner, independently of the child's global name.
    #[must_use]
    pub fn parent_key(&self) -> Option<&str> {
        self.layers
            .iter()
            .filter_map(|layer| attribute(layer.element(), "parentKey"))
            .next_back()
            .filter(|key| !key.is_empty())
    }

    /// Returns the parent arena index.
    #[must_use]
    pub const fn parent(&self) -> Option<usize> {
        self.parent
    }

    /// Returns direct children in construction order.
    #[must_use]
    pub fn children(&self) -> &[usize] {
        &self.children
    }

    /// Returns the owner that constructed this object from nested XML.
    ///
    /// This remains stable when a stock `parent` attribute later reparents the
    /// object for layout and frame ownership.
    #[must_use]
    pub const fn construction_parent(&self) -> Option<usize> {
        self.construction_parent
    }

    /// Returns nested objects in their original construction order.
    #[must_use]
    pub fn construction_children(&self) -> &[usize] {
        &self.construction_children
    }

    /// Returns unique inherited and concrete XML layers in application order.
    #[must_use]
    pub fn layers(&self) -> &[UiElementLayer<'bundle>] {
        &self.layers
    }
}

/// Complete nested UI object arena with expanded global names.
pub struct UiObjectTree<'bundle> {
    nodes: Vec<UiObjectNode<'bundle>>,
    by_name: HashMap<String, usize>,
    top_level: Vec<usize>,
    batches: Vec<UiObjectBatch>,
    pending_parents: Vec<(usize, String, AssetPath)>,
    dynamic_root: Option<usize>,
}

impl<'bundle> UiObjectTree<'bundle> {
    /// Instantiates every live declaration, inherited nested child, and region.
    ///
    /// # Errors
    ///
    /// Returns [`UiObjectError::Declaration`] for unavailable parents or
    /// templates, invalid `$parent` expansion, or incompatible global-name
    /// collisions. Names created inside an earlier template instance are
    /// registered before later roots resolve explicit `parent` attributes.
    pub fn from_catalog(
        catalog: &UiObjectCatalog<'bundle>,
        fonts: &FontCatalog,
    ) -> Result<Self, UiObjectError> {
        let mut tree = Self {
            nodes: Vec::new(),
            by_name: HashMap::new(),
            top_level: Vec::new(),
            batches: Vec::new(),
            pending_parents: Vec::new(),
            dynamic_root: None,
        };
        for definition in catalog.roots() {
            let first_node = tree.nodes.len();
            let root = tree.instantiate_root(catalog, fonts, definition)?;
            tree.batches.push(UiObjectBatch {
                action_index: definition.action_index(),
                root,
                first_node,
                node_count: tree.nodes.len() - first_node,
            });
        }
        tree.resolve_pending_parents()?;
        Ok(tree)
    }

    pub(crate) fn from_definition(
        catalog: &UiObjectCatalog<'bundle>,
        fonts: &FontCatalog,
        definition: &UiObjectDefinition<'bundle>,
    ) -> Result<Self, UiObjectError> {
        let mut tree = Self {
            nodes: Vec::new(),
            by_name: HashMap::new(),
            top_level: Vec::new(),
            batches: Vec::new(),
            pending_parents: Vec::new(),
            dynamic_root: None,
        };
        let first_node = tree.nodes.len();
        let root = tree.instantiate_root(catalog, fonts, definition)?;
        // A virtual definition is planned before its eventual CreateFrame
        // parent exists. Root-level `$parent` properties therefore remain
        // dynamic until that particular template instance is constructed.
        tree.dynamic_root = Some(root);
        tree.batches.push(UiObjectBatch {
            action_index: definition.action_index(),
            root,
            first_node,
            node_count: tree.nodes.len() - first_node,
        });
        Ok(tree)
    }

    pub(crate) fn pending_parent_name(&self, node_index: usize) -> Option<&str> {
        self.pending_parents
            .iter()
            .find(|(index, _, _)| *index == node_index)
            .map(|(_, name, _)| name.as_str())
    }

    pub(crate) fn is_dynamic_root(&self, node_index: usize) -> bool {
        self.dynamic_root == Some(node_index)
    }

    /// Returns all objects in construction order.
    #[must_use]
    pub fn nodes(&self) -> &[UiObjectNode<'bundle>] {
        &self.nodes
    }

    /// Returns top-level object indices in construction order.
    #[must_use]
    pub fn top_level(&self) -> &[usize] {
        &self.top_level
    }

    /// Returns live-root batches in exact expanded bundle-action order.
    #[must_use]
    pub fn batches(&self) -> &[UiObjectBatch] {
        &self.batches
    }

    /// Finds the live-root construction batch owned by one bundle action.
    #[must_use]
    pub fn batch_for_action(&self, action_index: usize) -> Option<UiObjectBatch> {
        self.batches
            .binary_search_by_key(&action_index, |batch| batch.action_index())
            .ok()
            .and_then(|index| self.batches.get(index).copied())
    }

    /// Finds an expanded global object name.
    #[must_use]
    pub fn node(&self, name: &str) -> Option<&UiObjectNode<'bundle>> {
        self.by_name
            .get(name)
            .and_then(|index| self.nodes.get(*index))
    }

    /// Returns the arena index registered for an expanded global name.
    #[must_use]
    pub fn node_index(&self, name: &str) -> Option<usize> {
        self.by_name.get(name).copied()
    }

    fn instantiate_root(
        &mut self,
        catalog: &UiObjectCatalog<'bundle>,
        fonts: &FontCatalog,
        definition: &UiObjectDefinition<'bundle>,
    ) -> Result<usize, UiObjectError> {
        let layers = definition_layers(catalog, definition)?;
        // Root template properties apply before the concrete declaration, so
        // an inherited `parent` remains effective unless a later layer replaces
        // it. OptionsFrameTemplate relies on this to sit above GlueParent.
        let requested_parent = layers
            .iter()
            .filter_map(|layer| attribute(layer.element, "parent"))
            .next_back()
            .filter(|name| !name.is_empty())
            .map(str::to_owned);
        let parent = requested_parent
            .as_deref()
            .and_then(|name| self.by_name.get(name).copied());
        let (node_index, first_new_layer) = self.create_or_merge(
            Some(definition.name().to_owned()),
            definition.kind(),
            UiObjectRole::Object,
            parent,
            None,
            layers,
        )?;
        if let Some(parent_name) = requested_parent.as_deref().filter(|_| parent.is_none()) {
            self.pending_parents.push((
                node_index,
                parent_name.to_owned(),
                definition.source_path().clone(),
            ));
        }
        self.instantiate_new_layers(catalog, fonts, node_index, first_new_layer)?;
        Ok(node_index)
    }

    fn instantiate_element(
        &mut self,
        catalog: &UiObjectCatalog<'bundle>,
        fonts: &FontCatalog,
        structural_parent: usize,
        layer: UiElementLayer<'bundle>,
        authored_kind: UiObjectKind,
        role: UiObjectRole,
    ) -> Result<(), UiObjectError> {
        let parent_name = self.nodes[structural_parent].name_context.as_deref();
        let name = attribute(layer.element, "name")
            .map(|value| expand_parent_name(layer.source_path, value, parent_name))
            .transpose()?;
        let requested_parent = attribute(layer.element, "parent")
            .map(|value| expand_parent_name(layer.source_path, value, parent_name))
            .transpose()?;
        let unresolved_parent = requested_parent
            .as_ref()
            .filter(|value| !self.by_name.contains_key(value.as_str()))
            .cloned();
        let parent = requested_parent
            .as_ref()
            .and_then(|value| self.by_name.get(value).copied())
            .unwrap_or(structural_parent);

        let mut layers = Vec::new();
        let mut kind = authored_kind;
        if let Some(parents) = attribute(layer.element, "inherits") {
            for template_name in parents.split(',').map(str::trim) {
                if let Some(template) = catalog.definition(template_name) {
                    if !template.virtual_object() {
                        return Err(object_error(
                            layer.source_path,
                            format!("nested object inherits non-virtual object {template_name}"),
                        ));
                    }
                    kind = inherited_object_kind(
                        layer.source_path,
                        name.as_deref().unwrap_or("<unnamed>"),
                        authored_kind,
                        kind,
                        template.kind(),
                    )?;
                    layers.extend(definition_layers(catalog, template)?);
                } else if authored_kind == UiObjectKind::FontString
                    && fonts.definition(template_name).is_some()
                {
                    // Font properties are resolved by the font-string stage and
                    // contribute no XML object layer here.
                } else {
                    return Err(UiObjectError::UnavailableTemplate {
                        path: layer.source_path.clone(),
                        object: name.clone().unwrap_or_else(|| "<unnamed>".to_owned()),
                        template: template_name.to_owned(),
                    });
                }
            }
        }
        layers.push(layer);
        let (node_index, first_new_layer) = self.create_or_merge(
            name,
            kind,
            role,
            Some(parent),
            Some(structural_parent),
            layers,
        )?;
        if let Some(parent_name) = unresolved_parent {
            self.pending_parents
                .push((node_index, parent_name, layer.source_path.clone()));
        }
        self.instantiate_new_layers(catalog, fonts, node_index, first_new_layer)
    }

    fn create_or_merge(
        &mut self,
        name: Option<String>,
        kind: UiObjectKind,
        role: UiObjectRole,
        parent: Option<usize>,
        construction_parent: Option<usize>,
        layers: Vec<UiElementLayer<'bundle>>,
    ) -> Result<(usize, usize), UiObjectError> {
        let existing_sibling = name.as_deref().and_then(|candidate| {
            let siblings = parent.map_or(self.top_level.as_slice(), |parent_index| {
                self.nodes[parent_index].children.as_slice()
            });
            siblings.iter().copied().find(|index| {
                let node = &self.nodes[*index];
                node.name.as_deref() == Some(candidate) && node.kind == kind && node.role == role
            })
        });
        if let Some(existing) = existing_sibling {
            let node = &mut self.nodes[existing];
            let first_new_layer = node.layers.len();
            for layer in layers {
                if !node
                    .layers
                    .iter()
                    .any(|existing| existing.same_source(layer))
                {
                    node.layers.push(layer);
                }
            }
            return Ok((existing, first_new_layer));
        }

        let node_index = self.nodes.len();
        let name_context = name.clone().or_else(|| {
            parent.and_then(|parent_index| self.nodes[parent_index].name_context.clone())
        });
        self.nodes.push(UiObjectNode {
            name: name.clone(),
            name_context,
            kind,
            role,
            parent,
            children: Vec::new(),
            construction_parent,
            construction_children: Vec::new(),
            layers,
        });
        if let Some(name) = name {
            self.by_name.insert(name, node_index);
        }
        if let Some(parent) = parent {
            self.nodes[parent].children.push(node_index);
        } else {
            self.top_level.push(node_index);
        }
        if let Some(parent) = construction_parent {
            self.nodes[parent].construction_children.push(node_index);
        }
        Ok((node_index, 0))
    }

    fn instantiate_new_layers(
        &mut self,
        catalog: &UiObjectCatalog<'bundle>,
        fonts: &FontCatalog,
        node_index: usize,
        first_new_layer: usize,
    ) -> Result<(), UiObjectError> {
        let layers = self.nodes[node_index].layers[first_new_layer..].to_vec();
        for layer in layers {
            self.scan_descendants(catalog, fonts, node_index, layer, layer.element)?;
        }
        Ok(())
    }

    fn scan_descendants(
        &mut self,
        catalog: &UiObjectCatalog<'bundle>,
        fonts: &FontCatalog,
        parent: usize,
        layer: UiElementLayer<'bundle>,
        element: &'bundle XmlElement,
    ) -> Result<(), UiObjectError> {
        for content in element.content() {
            let XmlContent::Element(index) = content else {
                continue;
            };
            let child = layer.document.element(*index).ok_or_else(|| {
                object_error(layer.source_path, "XML child index is outside the arena")
            })?;
            if let Some((kind, role)) = classify_child(child.name()) {
                self.instantiate_element(
                    catalog,
                    fonts,
                    parent,
                    UiElementLayer {
                        source_path: layer.source_path,
                        document: layer.document,
                        element: child,
                        draw_layer: layer.draw_layer,
                    },
                    kind,
                    role,
                )?;
            } else {
                let descendant_layer = if child.name() == "Layer" {
                    UiElementLayer {
                        draw_layer: Some(parse_draw_layer(layer.source_path, child)?),
                        ..layer
                    }
                } else {
                    layer
                };
                self.scan_descendants(catalog, fonts, parent, descendant_layer, child)?;
            }
        }
        Ok(())
    }

    fn resolve_pending_parents(&mut self) -> Result<(), UiObjectError> {
        for (node_index, parent_name, path) in self.pending_parents.drain(..) {
            let parent_index = self.by_name.get(&parent_name).copied().ok_or_else(|| {
                object_error(
                    &path,
                    format!(
                        "object {} names unavailable parent {parent_name}",
                        self.nodes[node_index]
                            .name
                            .as_deref()
                            .unwrap_or("<unnamed>")
                    ),
                )
            })?;
            if let Some(previous_parent) = self.nodes[node_index].parent {
                self.nodes[previous_parent]
                    .children
                    .retain(|index| *index != node_index);
            }
            self.nodes[node_index].parent = Some(parent_index);
            self.nodes[parent_index].children.push(node_index);
            self.top_level.retain(|index| *index != node_index);
        }
        Ok(())
    }
}

fn definition_layers<'bundle>(
    catalog: &UiObjectCatalog<'bundle>,
    definition: &UiObjectDefinition<'bundle>,
) -> Result<Vec<UiElementLayer<'bundle>>, UiObjectError> {
    definition
        .resolved_layers()
        .iter()
        .map(|index| {
            let layer = catalog.definition_at(*index).ok_or_else(|| {
                object_error(
                    definition.source_path(),
                    "resolved template index is outside the catalog",
                )
            })?;
            Ok(UiElementLayer {
                source_path: layer.source_path(),
                document: layer.document(),
                element: layer.element(),
                draw_layer: None,
            })
        })
        .collect()
}

fn parse_draw_layer(path: &AssetPath, element: &XmlElement) -> Result<UiDrawLayer, UiObjectError> {
    match attribute(element, "level") {
        Some("BACKGROUND") => Ok(UiDrawLayer::Background),
        Some("BORDER") => Ok(UiDrawLayer::Border),
        Some("ARTWORK") | None => Ok(UiDrawLayer::Artwork),
        Some("OVERLAY") => Ok(UiDrawLayer::Overlay),
        // Build 12340's BNet.xml contains this exact typo around its glow
        // texture. Keep the compatibility rule exact rather than accepting
        // arbitrary trailing characters as an invented fallback.
        Some("OVERLAY`") => Ok(UiDrawLayer::Overlay),
        Some("HIGHLIGHT") => Ok(UiDrawLayer::Highlight),
        Some(value) => Err(object_error(
            path,
            format!("unsupported draw layer {value}"),
        )),
    }
}

fn classify_child(name: &str) -> Option<(UiObjectKind, UiObjectRole)> {
    let alias = match name {
        "ButtonText" => Some((UiObjectKind::FontString, UiObjectRole::ButtonText)),
        "NormalTexture" => Some((UiObjectKind::Texture, UiObjectRole::NormalTexture)),
        "PushedTexture" => Some((UiObjectKind::Texture, UiObjectRole::PushedTexture)),
        "DisabledTexture" => Some((UiObjectKind::Texture, UiObjectRole::DisabledTexture)),
        "HighlightTexture" => Some((UiObjectKind::Texture, UiObjectRole::HighlightTexture)),
        "CheckedTexture" => Some((UiObjectKind::Texture, UiObjectRole::CheckedTexture)),
        "DisabledCheckedTexture" => {
            Some((UiObjectKind::Texture, UiObjectRole::DisabledCheckedTexture))
        }
        "ThumbTexture" => Some((UiObjectKind::Texture, UiObjectRole::ThumbTexture)),
        _ => None,
    };
    alias.or_else(|| UiObjectKind::from_element_name(name).map(|kind| (kind, UiObjectRole::Object)))
}

fn expand_parent_name(
    path: &AssetPath,
    value: &str,
    parent_name: Option<&str>,
) -> Result<String, UiObjectError> {
    if !value.contains("$parent") {
        return Ok(value.to_owned());
    }
    let parent_name = parent_name.ok_or_else(|| {
        object_error(
            path,
            format!("name {value} requires an unnamed structural parent"),
        )
    })?;
    Ok(value.replace("$parent", parent_name))
}

fn attribute<'a>(element: &'a XmlElement, name: &str) -> Option<&'a str> {
    element
        .attributes()
        .iter()
        .find(|attribute| attribute.name() == name)
        .map(|attribute| attribute.value())
}

fn object_error(path: &AssetPath, message: impl Into<String>) -> UiObjectError {
    UiObjectError::Declaration {
        path: path.clone(),
        message: message.into(),
    }
}
