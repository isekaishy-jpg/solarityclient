//! Ordered global templates and live root object declarations.

use std::collections::HashMap;

use solarity_asset::AssetPath;

use crate::frame::UiObjectError;
use crate::{FontCatalog, UiBundle, UiLoadAction, UiResourceContent, XmlDocument, XmlElement};

/// Concrete root object types present in stock GlueXML and FrameXML.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiObjectKind {
    /// Generic frame.
    Frame,
    /// Clickable button.
    Button,
    /// Checkable button.
    CheckButton,
    /// Color selection widget.
    ColorSelect,
    /// Cooldown spiral widget.
    Cooldown,
    /// Editable text field.
    EditBox,
    /// Rendered text region.
    FontString,
    /// Game tooltip frame.
    GameTooltip,
    /// Message display frame.
    MessageFrame,
    /// Minimap presentation frame.
    Minimap,
    /// Quest objective polygon and hit-test frame used by the world map.
    QuestPoiFrame,
    /// 3D model frame.
    Model,
    /// Glue model frame with the stock FFX presentation path.
    ModelFfx,
    /// Movie playback frame.
    MovieFrame,
    /// Scrolling viewport frame.
    ScrollFrame,
    /// Scrolling message display frame.
    ScrollingMessageFrame,
    /// Restricted HTML frame.
    SimpleHtml,
    /// Slider control.
    Slider,
    /// Status bar control.
    StatusBar,
    /// Textured render region.
    Texture,
    /// World presentation root.
    WorldFrame,
}

impl UiObjectKind {
    pub(crate) fn from_element_name(name: &str) -> Option<Self> {
        [
            ("Frame", Self::Frame),
            ("Button", Self::Button),
            ("CheckButton", Self::CheckButton),
            ("ColorSelect", Self::ColorSelect),
            ("Cooldown", Self::Cooldown),
            ("EditBox", Self::EditBox),
            ("FontString", Self::FontString),
            ("GameTooltip", Self::GameTooltip),
            ("MessageFrame", Self::MessageFrame),
            ("Minimap", Self::Minimap),
            ("QuestPOIFrame", Self::QuestPoiFrame),
            ("Model", Self::Model),
            ("ModelFFX", Self::ModelFfx),
            ("MovieFrame", Self::MovieFrame),
            ("ScrollFrame", Self::ScrollFrame),
            ("ScrollingMessageFrame", Self::ScrollingMessageFrame),
            ("SimpleHTML", Self::SimpleHtml),
            ("Slider", Self::Slider),
            ("StatusBar", Self::StatusBar),
            ("Texture", Self::Texture),
            ("WorldFrame", Self::WorldFrame),
        ]
        .into_iter()
        .find_map(|(stock_name, kind)| name.eq_ignore_ascii_case(stock_name).then_some(kind))
    }
}

/// A resolved direct inheritance target in stock declaration order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiInheritanceTarget {
    /// An earlier virtual UI object by definition index.
    Object(usize),
    /// A global font object inherited by a root `FontString`.
    Font(String),
}

/// One named root declaration retaining its original XML subtree.
pub struct UiObjectDefinition<'bundle> {
    action_index: usize,
    kind: UiObjectKind,
    name: String,
    virtual_object: bool,
    parent_name: Option<String>,
    inherited_from: Vec<UiInheritanceTarget>,
    resolved_layers: Vec<usize>,
    source_path: &'bundle AssetPath,
    document: &'bundle XmlDocument,
    element: &'bundle XmlElement,
}

impl<'bundle> UiObjectDefinition<'bundle> {
    /// Returns the expanded bundle action that registers this declaration.
    #[must_use]
    pub const fn action_index(&self) -> usize {
        self.action_index
    }

    /// Returns the concrete stock object type.
    #[must_use]
    pub const fn kind(&self) -> UiObjectKind {
        self.kind
    }

    /// Returns the global XML name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns whether this declaration is a template rather than a live root.
    #[must_use]
    pub const fn virtual_object(&self) -> bool {
        self.virtual_object
    }

    /// Returns an explicit global runtime parent name.
    #[must_use]
    pub fn parent_name(&self) -> Option<&str> {
        self.parent_name.as_deref()
    }

    /// Returns direct inheritance targets in their XML order.
    #[must_use]
    pub fn inherited_from(&self) -> &[UiInheritanceTarget] {
        &self.inherited_from
    }

    /// Returns ancestor template definition indices followed by this object.
    #[must_use]
    pub fn resolved_layers(&self) -> &[usize] {
        &self.resolved_layers
    }

    /// Returns the XML asset containing this declaration.
    #[must_use]
    pub const fn source_path(&self) -> &'bundle AssetPath {
        self.source_path
    }

    /// Returns the owning document used to resolve child arena indices.
    #[must_use]
    pub const fn document(&self) -> &'bundle XmlDocument {
        self.document
    }

    /// Returns the original root XML element.
    #[must_use]
    pub const fn element(&self) -> &'bundle XmlElement {
        self.element
    }
}

/// Ordered global templates and live root declarations for one UI bundle.
pub struct UiObjectCatalog<'bundle> {
    definitions: Vec<UiObjectDefinition<'bundle>>,
    by_name: HashMap<String, usize>,
    template_indices: Vec<usize>,
    root_indices: Vec<usize>,
}

impl<'bundle> UiObjectCatalog<'bundle> {
    /// Registers every non-font XML action in exact expanded load order.
    ///
    /// # Errors
    ///
    /// Returns [`UiObjectError::Declaration`] for unknown root types, missing
    /// names, invalid booleans, duplicate globals, or unavailable inheritance
    /// targets. Object parents must be earlier virtual declarations; a
    /// `FontString` may additionally inherit an earlier global font object.
    pub fn from_bundle(
        bundle: &'bundle UiBundle,
        fonts: &FontCatalog,
    ) -> Result<Self, UiObjectError> {
        let mut catalog = Self {
            definitions: Vec::new(),
            by_name: HashMap::new(),
            template_indices: Vec::new(),
            root_indices: Vec::new(),
        };

        for (action_index, action) in bundle.actions().iter().enumerate() {
            let UiLoadAction::XmlElement {
                resource_index,
                element_index,
            } = action
            else {
                continue;
            };
            let resource = bundle.resource(*resource_index).ok_or_else(|| {
                declaration_error(
                    bundle.manifest().path(),
                    "UI action resource index is outside the bundle",
                )
            })?;
            let UiResourceContent::Xml(document) = resource.content() else {
                return Err(declaration_error(
                    resource.path(),
                    "XML action refers to a non-XML resource",
                ));
            };
            let element = document.element(*element_index).ok_or_else(|| {
                declaration_error(
                    resource.path(),
                    "XML action element index is outside the arena",
                )
            })?;
            if element.name() == "Font" {
                continue;
            }
            catalog.register(action_index, resource.path(), document, element, fonts)?;
        }
        Ok(catalog)
    }

    /// Returns all object declarations in registration order.
    #[must_use]
    pub fn definitions(&self) -> &[UiObjectDefinition<'bundle>] {
        &self.definitions
    }

    /// Returns a named declaration.
    #[must_use]
    pub fn definition(&self, name: &str) -> Option<&UiObjectDefinition<'bundle>> {
        self.by_name
            .get(name)
            .and_then(|index| self.definitions.get(*index))
    }

    /// Returns a declaration by its stable registration index.
    #[must_use]
    pub fn definition_at(&self, index: usize) -> Option<&UiObjectDefinition<'bundle>> {
        self.definitions.get(index)
    }

    /// Iterates virtual template declarations in registration order.
    pub fn templates(&self) -> impl ExactSizeIterator<Item = &UiObjectDefinition<'bundle>> {
        self.template_indices
            .iter()
            .map(|index| &self.definitions[*index])
    }

    /// Iterates live root declarations in registration order.
    pub fn roots(&self) -> impl ExactSizeIterator<Item = &UiObjectDefinition<'bundle>> {
        self.root_indices
            .iter()
            .map(|index| &self.definitions[*index])
    }

    fn register(
        &mut self,
        action_index: usize,
        path: &'bundle AssetPath,
        document: &'bundle XmlDocument,
        element: &'bundle XmlElement,
        fonts: &FontCatalog,
    ) -> Result<(), UiObjectError> {
        let authored_kind = UiObjectKind::from_element_name(element.name()).ok_or_else(|| {
            declaration_error(path, format!("unsupported root type {}", element.name()))
        })?;
        let name = attribute(element, "name")
            .ok_or_else(|| declaration_error(path, format!("{} has no name", element.name())))?
            .to_owned();
        if self.by_name.contains_key(&name) {
            return Err(declaration_error(
                path,
                format!("duplicate global object name {name}"),
            ));
        }

        let virtual_object = attribute(element, "virtual")
            .map(|value| parse_bool(path, "virtual", value))
            .transpose()?
            .unwrap_or(false);
        let mut inherited_from = Vec::new();
        let mut resolved_layers = Vec::new();
        let mut kind = authored_kind;
        if let Some(parents) = attribute(element, "inherits") {
            for parent_name in parents.split(',').map(str::trim) {
                if let Some(index) = self.by_name.get(parent_name).copied() {
                    if !self.definitions[index].virtual_object {
                        return Err(declaration_error(
                            path,
                            format!("object {name} inherits non-virtual object {parent_name}"),
                        ));
                    }
                    kind = inherited_object_kind(
                        path,
                        &name,
                        authored_kind,
                        kind,
                        self.definitions[index].kind(),
                    )?;
                    resolved_layers.extend_from_slice(self.definitions[index].resolved_layers());
                    inherited_from.push(UiInheritanceTarget::Object(index));
                } else if authored_kind == UiObjectKind::FontString
                    && fonts.definition(parent_name).is_some()
                {
                    inherited_from.push(UiInheritanceTarget::Font(parent_name.to_owned()));
                } else {
                    return Err(declaration_error(
                        path,
                        format!("object {name} inherits unavailable object {parent_name}"),
                    ));
                }
            }
        }

        let index = self.definitions.len();
        resolved_layers.push(index);
        let definition = UiObjectDefinition {
            action_index,
            kind,
            name: name.clone(),
            virtual_object,
            parent_name: attribute(element, "parent").map(str::to_owned),
            inherited_from,
            resolved_layers,
            source_path: path,
            document,
            element,
        };
        self.by_name.insert(name, index);
        self.definitions.push(definition);
        if virtual_object {
            self.template_indices.push(index);
        } else {
            self.root_indices.push(index);
        }
        Ok(())
    }
}

/// Resolves stock's generic `<Frame inherits="ConcreteTemplate">` factory form.
///
/// FrameXML uses this for `BasicHybridScrollFrameTemplateScrollBar`: the
/// authored element is `Frame`, while `HybridScrollBarTemplate` is a `Slider`
/// and supplies `OnValueChanged`. A non-generic tag remains authoritative, and
/// incompatible concrete templates fail instead of choosing one implicitly.
pub(crate) fn inherited_object_kind(
    path: &AssetPath,
    object: &str,
    authored: UiObjectKind,
    current: UiObjectKind,
    inherited: UiObjectKind,
) -> Result<UiObjectKind, UiObjectError> {
    if authored != UiObjectKind::Frame || inherited == UiObjectKind::Frame {
        return Ok(current);
    }
    if current == UiObjectKind::Frame || current == inherited {
        return Ok(inherited);
    }
    Err(declaration_error(
        path,
        format!(
            "generic frame {object} inherits incompatible concrete types {current:?} and {inherited:?}"
        ),
    ))
}

fn attribute<'a>(element: &'a XmlElement, name: &str) -> Option<&'a str> {
    element
        .attributes()
        .iter()
        .find(|attribute| attribute.name() == name)
        .map(|attribute| attribute.value())
}

fn parse_bool(path: &AssetPath, field: &str, value: &str) -> Result<bool, UiObjectError> {
    if value.eq_ignore_ascii_case("true") {
        Ok(true)
    } else if value.eq_ignore_ascii_case("false") {
        Ok(false)
    } else {
        Err(declaration_error(
            path,
            format!("invalid {field} boolean {value}"),
        ))
    }
}

fn declaration_error(path: &AssetPath, message: impl Into<String>) -> UiObjectError {
    UiObjectError::Declaration {
        path: path.clone(),
        message: message.into(),
    }
}
