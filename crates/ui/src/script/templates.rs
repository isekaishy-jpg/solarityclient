//! Owned runtime prototypes for XML templates used by `CreateFrame`.

use mlua::{Lua, RegistryKey};

use crate::{
    FontCatalog, HorizontalJustification, UiLayoutPlan, UiObjectCatalog, UiObjectKind,
    UiObjectTree, UiPoint, UiScriptError, UiScriptHandler, UiScriptPlan, UiScriptTarget,
    UiTexturePlan, UiTextureStatePlan, VerticalJustification,
};

const POINT_COUNT: usize = 9;

/// One named XML template and its contiguous runtime-prototype range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiRuntimeTemplate {
    name: String,
    root: usize,
    first_node: usize,
    node_count: usize,
}

impl UiRuntimeTemplate {
    /// Returns the stock global template name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the root prototype node index.
    #[must_use]
    pub const fn root(&self) -> usize {
        self.root
    }

    /// Returns the number of objects constructed by this template.
    #[must_use]
    pub const fn node_count(&self) -> usize {
        self.node_count
    }

    pub(super) fn node_range(&self) -> std::ops::Range<usize> {
        self.first_node..self.first_node + self.node_count
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum UiRuntimeName {
    Root,
    RootSuffix(String),
    Absolute(String),
}

/// One owned object prototype independent of the source XML lifetime.
#[derive(Clone, Debug, PartialEq)]
pub struct UiRuntimeTemplateNode {
    name: Option<UiRuntimeName>,
    kind: UiObjectKind,
    role: crate::UiObjectRole,
    parent: Option<usize>,
    external_parent: Option<String>,
    construction_children: Vec<usize>,
    dimensions: (f64, f64),
    shown: bool,
    alpha: f64,
    scale: f64,
    anchors: Vec<UiRuntimeAnchorPrototype>,
    font_assigned: bool,
    font_object_name: Option<String>,
    justify_h: String,
    justify_v: String,
    texture_coords: [f64; 8],
    texture_colors: [[f64; 4]; 4],
    script_targets: Vec<(UiScriptHandler, UiScriptTarget)>,
}

#[derive(Clone, Debug, PartialEq)]
struct UiRuntimeAnchorPrototype {
    point: UiPoint,
    target: UiRuntimeAnchorTarget,
    relative_point: UiPoint,
    offset: (f64, f64),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum UiRuntimeAnchorTarget {
    Parent,
    Object(usize),
    Global(String),
}

impl UiRuntimeTemplateNode {
    /// Returns the concrete widget kind created for this node.
    #[must_use]
    pub const fn kind(&self) -> UiObjectKind {
        self.kind
    }

    /// Returns the prototype parent index within the flat plan.
    #[must_use]
    pub const fn parent(&self) -> Option<usize> {
        self.parent
    }

    /// Returns a global parent that must be resolved when the template runs.
    #[must_use]
    pub fn external_parent(&self) -> Option<&str> {
        self.external_parent.as_deref()
    }

    /// Returns direct structural children in construction order.
    #[must_use]
    pub fn construction_children(&self) -> &[usize] {
        &self.construction_children
    }

    /// Returns the resolved initial width and height.
    #[must_use]
    pub const fn dimensions(&self) -> (f64, f64) {
        self.dimensions
    }

    /// Returns the resolved local startup visibility.
    #[must_use]
    pub const fn shown(&self) -> bool {
        self.shown
    }

    pub(super) fn name(&self) -> Option<&UiRuntimeName> {
        self.name.as_ref()
    }
}

/// Owned prototypes and compiled `OnLoad` functions for dynamic UI creation.
pub struct UiRuntimeTemplatePlan {
    templates: Vec<UiRuntimeTemplate>,
    nodes: Vec<UiRuntimeTemplateNode>,
    functions: Vec<RegistryKey>,
}

pub(super) const TEMPLATE_REGISTRY: &str = "solarity.ui.runtime_templates";

impl UiRuntimeTemplatePlan {
    /// Expands every virtual template into an owned dynamic construction plan.
    ///
    /// # Errors
    ///
    /// Returns [`UiScriptError::Template`] when template construction, layout,
    /// callback compilation, or registry retention fails.
    pub fn from_catalog(
        catalog: &UiObjectCatalog<'_>,
        fonts: &FontCatalog,
        lua: &Lua,
    ) -> Result<Self, UiScriptError> {
        let mut plan = Self {
            templates: Vec::new(),
            nodes: Vec::new(),
            functions: Vec::new(),
        };
        for definition in catalog.templates() {
            let template_name = definition.name();
            let tree = UiObjectTree::from_definition(catalog, fonts, definition)
                .map_err(|error| template_error(template_name, error))?;
            let layout = UiLayoutPlan::from_tree(&tree)
                .map_err(|error| template_error(template_name, error))?;
            let scripts = UiScriptPlan::from_tree(&tree, lua)
                .map_err(|error| template_error(template_name, error))?;
            let textures = UiTexturePlan::from_tree(&tree)
                .map_err(|error| template_error(template_name, error))?;
            let texture_states = UiTextureStatePlan::resolve(&tree, &textures)
                .map_err(|error| template_error(template_name, error))?;
            let first_node = plan.nodes.len();
            for (local_index, object) in tree.nodes().iter().enumerate() {
                let region =
                    resolve_local_region(&tree, &layout, local_index, first_node, template_name)?;
                let script_node =
                    scripts
                        .node(local_index)
                        .ok_or_else(|| UiScriptError::Template {
                            template: template_name.to_owned(),
                            message: format!("object {local_index} has no script state"),
                        })?;
                let script_targets = scripts
                    .bindings_for(script_node)
                    .iter()
                    .map(|binding| {
                        remap_target(
                            &mut plan.functions,
                            &scripts,
                            lua,
                            binding.target(),
                            template_name,
                        )
                        .map(|target| (binding.handler(), target))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let (font_assigned, font_object_name, justify_h, justify_v) =
                    initial_font(object, fonts);
                let texture_coords = initial_texture_coords(&tree, &texture_states, local_index)
                    .map_err(|error| template_error(template_name, error))?;
                let texture_colors = initial_texture_colors(&tree, &texture_states, local_index)
                    .map_err(|error| template_error(template_name, error))?;
                plan.nodes.push(UiRuntimeTemplateNode {
                    name: runtime_name(template_name, object.name()),
                    kind: object.kind(),
                    role: object.role(),
                    parent: object.parent().map(|index| first_node + index),
                    external_parent: tree.pending_parent_name(local_index).map(str::to_owned),
                    construction_children: object
                        .construction_children()
                        .iter()
                        .map(|index| first_node + index)
                        .collect(),
                    dimensions: region.dimensions,
                    shown: region.shown,
                    alpha: region.alpha,
                    scale: region.scale,
                    anchors: region.anchors,
                    font_assigned,
                    font_object_name,
                    justify_h,
                    justify_v,
                    texture_coords,
                    texture_colors,
                    script_targets,
                });
            }
            plan.templates.push(UiRuntimeTemplate {
                name: template_name.to_owned(),
                root: first_node,
                first_node,
                node_count: tree.nodes().len(),
            });
        }
        Ok(plan)
    }

    /// Returns templates in stock declaration order.
    #[must_use]
    pub fn templates(&self) -> &[UiRuntimeTemplate] {
        &self.templates
    }

    /// Returns one prototype node by flat index.
    #[must_use]
    pub fn node(&self, index: usize) -> Option<&UiRuntimeTemplateNode> {
        self.nodes.get(index)
    }

    /// Returns the number of retained prototype nodes.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub(super) fn compiled_function(&self, lua: &Lua, index: u32) -> mlua::Result<mlua::Function> {
        let key = self.functions.get(index as usize).ok_or_else(|| {
            mlua::Error::runtime("runtime template function index is outside the arena")
        })?;
        lua.registry_value(key)
    }

    pub(super) fn install(&self, lua: &Lua) -> mlua::Result<()> {
        let templates = lua.create_table()?;
        for template in &self.templates {
            let descriptor = lua.create_table()?;
            descriptor.raw_set("root", template.root - template.first_node + 1)?;
            let nodes = lua.create_table()?;
            for (local_index, global_index) in template.node_range().enumerate() {
                let node = self.nodes.get(global_index).ok_or_else(|| {
                    mlua::Error::runtime("runtime template node range is outside the arena")
                })?;
                let record = lua.create_table()?;
                record.raw_set("kind", object_kind_name(node.kind))?;
                record.raw_set("role", object_role_name(node.role))?;
                match node.name() {
                    Some(UiRuntimeName::Root) => record.raw_set("root_name", true)?,
                    Some(UiRuntimeName::RootSuffix(suffix)) => {
                        record.raw_set("root_suffix", suffix.as_str())?;
                    }
                    Some(UiRuntimeName::Absolute(name)) => {
                        record.raw_set("absolute_name", name.as_str())?;
                    }
                    None => {}
                }
                if let Some(parent) = node.parent {
                    let local_parent =
                        parent.checked_sub(template.first_node).ok_or_else(|| {
                            mlua::Error::runtime(
                                "runtime template parent precedes its template range",
                            )
                        })?;
                    record.raw_set("parent", local_parent + 1)?;
                }
                if let Some(parent) = &node.external_parent {
                    record.raw_set("external_parent", parent.as_str())?;
                }
                let children = node
                    .construction_children
                    .iter()
                    .map(|child| {
                        child
                            .checked_sub(template.first_node)
                            .map(|index| index + 1)
                            .ok_or_else(|| {
                                mlua::Error::runtime(
                                    "runtime template child precedes its template range",
                                )
                            })
                    })
                    .collect::<mlua::Result<Vec<_>>>()?;
                record.raw_set("children", lua.create_sequence_from(children)?)?;
                record.raw_set("width", node.dimensions.0)?;
                record.raw_set("height", node.dimensions.1)?;
                record.raw_set("shown", node.shown)?;
                record.raw_set("alpha", node.alpha)?;
                record.raw_set("scale", node.scale)?;
                let anchors = lua.create_table()?;
                for anchor in &node.anchors {
                    let anchor_record = lua.create_table()?;
                    anchor_record.raw_set("point", point_name(anchor.point))?;
                    anchor_record.raw_set("relative_point", point_name(anchor.relative_point))?;
                    anchor_record.raw_set("x", anchor.offset.0)?;
                    anchor_record.raw_set("y", anchor.offset.1)?;
                    match &anchor.target {
                        UiRuntimeAnchorTarget::Parent => {
                            anchor_record.raw_set("target_kind", "parent")?;
                        }
                        UiRuntimeAnchorTarget::Object(index) => {
                            let local_target =
                                index.checked_sub(template.first_node).ok_or_else(|| {
                                    mlua::Error::runtime(
                                        "runtime template anchor target precedes its template",
                                    )
                                })? + 1;
                            anchor_record.raw_set("target_kind", "local")?;
                            anchor_record.raw_set("target", local_target)?;
                        }
                        UiRuntimeAnchorTarget::Global(name) => {
                            anchor_record.raw_set("target_kind", "global")?;
                            anchor_record.raw_set("target", name.as_str())?;
                        }
                    }
                    anchors.raw_set(anchor.point.index() + 1, anchor_record)?;
                }
                record.raw_set("anchors", anchors)?;
                record.raw_set("font_assigned", node.font_assigned)?;
                if let Some(name) = &node.font_object_name {
                    record.raw_set("font_object_name", name.as_str())?;
                }
                record.raw_set("justify_h", node.justify_h.as_str())?;
                record.raw_set("justify_v", node.justify_v.as_str())?;
                record.raw_set(
                    "texture_coords",
                    lua.create_sequence_from(node.texture_coords)?,
                )?;
                record.raw_set(
                    "texture_color",
                    lua.create_sequence_from(node.texture_colors.iter().flatten().copied())?,
                )?;
                let scripts = lua.create_table()?;
                for (handler, target) in &node.script_targets {
                    match target {
                        UiScriptTarget::Compiled(index) => {
                            scripts
                                .raw_set(handler.name(), self.compiled_function(lua, *index)?)?;
                        }
                        UiScriptTarget::Global(name) => {
                            scripts.raw_set(handler.name(), name.as_str())?;
                        }
                    }
                }
                record.raw_set("scripts", scripts)?;
                nodes.raw_set(local_index + 1, record)?;
            }
            descriptor.raw_set("nodes", nodes)?;
            templates.raw_set(template.name.as_str(), descriptor)?;
        }
        lua.set_named_registry_value(TEMPLATE_REGISTRY, templates)
    }
}

fn initial_font(
    node: &crate::UiObjectNode<'_>,
    fonts: &FontCatalog,
) -> (bool, Option<String>, String, String) {
    if node.kind() != UiObjectKind::FontString {
        return (false, None, "CENTER".to_owned(), "MIDDLE".to_owned());
    }
    let mut assigned = false;
    let mut object_name = None;
    let mut justify_h = "CENTER".to_owned();
    let mut justify_v = "MIDDLE".to_owned();
    for layer in node.layers() {
        if let Some(inherits) = attribute(layer.element(), "inherits") {
            for name in inherits.split(',').map(str::trim) {
                if let Some(definition) = fonts.definition(name) {
                    assigned = true;
                    object_name = Some(name.to_owned());
                    apply_justification(definition, &mut justify_h, &mut justify_v);
                }
            }
        }
        if let Some(font) = attribute(layer.element(), "font") {
            assigned = !font.is_empty();
            let definition = fonts.definition(font);
            object_name = definition.map(|definition| definition.name().to_owned());
            if let Some(definition) = definition {
                apply_justification(definition, &mut justify_h, &mut justify_v);
            }
        }
        if let Some(value) = attribute(layer.element(), "justifyH") {
            justify_h = value.to_ascii_uppercase();
        }
        if let Some(value) = attribute(layer.element(), "justifyV") {
            justify_v = value.to_ascii_uppercase();
        }
    }
    (assigned, object_name, justify_h, justify_v)
}

fn apply_justification(
    definition: &crate::FontDefinition,
    justify_h: &mut String,
    justify_v: &mut String,
) {
    if let Some(value) = definition.horizontal_justification() {
        *justify_h = match value {
            HorizontalJustification::Left => "LEFT",
            HorizontalJustification::Center => "CENTER",
            HorizontalJustification::Right => "RIGHT",
        }
        .to_owned();
    }
    if let Some(value) = definition.vertical_justification() {
        *justify_v = match value {
            VerticalJustification::Top => "TOP",
            VerticalJustification::Middle => "MIDDLE",
            VerticalJustification::Bottom => "BOTTOM",
        }
        .to_owned();
    }
}

fn initial_texture_coords(
    tree: &UiObjectTree<'_>,
    textures: &UiTextureStatePlan,
    index: usize,
) -> Result<[f64; 8], &'static str> {
    let coords = [0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0];
    if tree.nodes().get(index).map(|node| node.kind()) != Some(UiObjectKind::Texture) {
        return Ok(coords);
    }
    textures
        .state(index)
        .map(|state| state.tex_coords().map(f64::from))
        .ok_or("texture is outside the template texture state plan")
}

fn initial_texture_colors(
    tree: &UiObjectTree<'_>,
    textures: &UiTextureStatePlan,
    index: usize,
) -> Result<[[f64; 4]; 4], &'static str> {
    let colors = [[1.0, 1.0, 1.0, 1.0]; 4];
    if tree.nodes().get(index).map(|node| node.kind()) != Some(UiObjectKind::Texture) {
        return Ok(colors);
    }
    textures
        .state(index)
        .map(|state| state.vertex_colors().map(|color| color.map(f64::from)))
        .ok_or("texture is outside the template texture state plan")
}

fn attribute<'a>(element: &'a crate::XmlElement, name: &str) -> Option<&'a str> {
    element
        .attributes()
        .iter()
        .find(|attribute| attribute.name() == name)
        .map(|attribute| attribute.value())
}

fn object_kind_name(kind: UiObjectKind) -> &'static str {
    match kind {
        UiObjectKind::Frame => "Frame",
        UiObjectKind::Button => "Button",
        UiObjectKind::CheckButton => "CheckButton",
        UiObjectKind::ColorSelect => "ColorSelect",
        UiObjectKind::Cooldown => "Cooldown",
        UiObjectKind::EditBox => "EditBox",
        UiObjectKind::FontString => "FontString",
        UiObjectKind::GameTooltip => "GameTooltip",
        UiObjectKind::MessageFrame => "MessageFrame",
        UiObjectKind::Minimap => "Minimap",
        UiObjectKind::Model => "Model",
        UiObjectKind::ModelFfx => "ModelFFX",
        UiObjectKind::MovieFrame => "MovieFrame",
        UiObjectKind::ScrollFrame => "ScrollFrame",
        UiObjectKind::ScrollingMessageFrame => "ScrollingMessageFrame",
        UiObjectKind::SimpleHtml => "SimpleHTML",
        UiObjectKind::Slider => "Slider",
        UiObjectKind::StatusBar => "StatusBar",
        UiObjectKind::Texture => "Texture",
        UiObjectKind::WorldFrame => "WorldFrame",
    }
}

fn object_role_name(role: crate::UiObjectRole) -> &'static str {
    match role {
        crate::UiObjectRole::Object => "object",
        crate::UiObjectRole::ButtonText => "button_text",
        crate::UiObjectRole::NormalTexture => "normal_texture",
        crate::UiObjectRole::PushedTexture => "pushed_texture",
        crate::UiObjectRole::DisabledTexture => "disabled_texture",
        crate::UiObjectRole::HighlightTexture => "highlight_texture",
        crate::UiObjectRole::CheckedTexture => "checked_texture",
        crate::UiObjectRole::DisabledCheckedTexture => "disabled_checked_texture",
        crate::UiObjectRole::ThumbTexture => "thumb_texture",
    }
}

struct ResolvedLocalRegion {
    dimensions: (f64, f64),
    shown: bool,
    alpha: f64,
    scale: f64,
    anchors: Vec<UiRuntimeAnchorPrototype>,
}

fn resolve_local_region(
    tree: &UiObjectTree<'_>,
    layout: &UiLayoutPlan,
    node_index: usize,
    first_node: usize,
    template: &str,
) -> Result<ResolvedLocalRegion, UiScriptError> {
    let node = layout
        .node(node_index)
        .ok_or_else(|| UiScriptError::Template {
            template: template.to_owned(),
            message: format!("object {node_index} has no layout-plan entry"),
        })?;
    let mut width = 0.0;
    let mut height = 0.0;
    let mut shown = true;
    let mut alpha = 1.0;
    let mut scale = 1.0;
    let mut anchors: [Option<UiRuntimeAnchorPrototype>; POINT_COUNT] =
        std::array::from_fn(|_| None);
    for layer in layout.layers_for(node) {
        if let Some(dimensions) = layer.dimensions() {
            if let Some(value) = dimensions.width() {
                width = f64::from(value);
            }
            if let Some(value) = dimensions.height() {
                height = f64::from(value);
            }
        }
        if let Some(hidden) = layer.hidden() {
            shown = !hidden;
        }
        if layer.anchors_present() {
            for anchor in layout.anchors_for(*layer) {
                let target = match anchor.relative_to() {
                    Some(name) => tree.node_index(name).map_or_else(
                        || UiRuntimeAnchorTarget::Global(name.to_owned()),
                        |index| UiRuntimeAnchorTarget::Object(first_node + index),
                    ),
                    None => tree.nodes()[node_index]
                        .parent()
                        .map_or(UiRuntimeAnchorTarget::Parent, |index| {
                            UiRuntimeAnchorTarget::Object(first_node + index)
                        }),
                };
                anchors[anchor.point().index()] = Some(UiRuntimeAnchorPrototype {
                    point: anchor.point(),
                    target,
                    relative_point: anchor.relative_point().unwrap_or(anchor.point()),
                    offset: anchor.offset().map_or((0.0, 0.0), |offset| {
                        (f64::from(offset.0), f64::from(offset.1))
                    }),
                });
            }
        } else if layer.set_all_points() == Some(true) {
            let target = tree.nodes()[node_index]
                .parent()
                .map_or(UiRuntimeAnchorTarget::Parent, |index| {
                    UiRuntimeAnchorTarget::Object(first_node + index)
                });
            anchors = std::array::from_fn(|_| None);
            anchors[UiPoint::TopLeft.index()] = Some(UiRuntimeAnchorPrototype {
                point: UiPoint::TopLeft,
                target: target.clone(),
                relative_point: UiPoint::TopLeft,
                offset: (0.0, 0.0),
            });
            anchors[UiPoint::BottomRight.index()] = Some(UiRuntimeAnchorPrototype {
                point: UiPoint::BottomRight,
                target,
                relative_point: UiPoint::BottomRight,
                offset: (0.0, 0.0),
            });
        }
        if let Some(value) = layer.alpha() {
            alpha = f64::from(value);
        }
        if let Some(value) = layer.scale() {
            if value <= 0.0 {
                return Err(UiScriptError::Template {
                    template: template.to_owned(),
                    message: format!("object {node_index} has nonpositive scale {value}"),
                });
            }
            scale = f64::from(value);
        }
    }
    Ok(ResolvedLocalRegion {
        dimensions: (width, height),
        shown,
        alpha,
        scale,
        anchors: anchors.into_iter().flatten().collect(),
    })
}

const fn point_name(point: UiPoint) -> &'static str {
    match point {
        UiPoint::TopLeft => "TOPLEFT",
        UiPoint::Top => "TOP",
        UiPoint::TopRight => "TOPRIGHT",
        UiPoint::Left => "LEFT",
        UiPoint::Center => "CENTER",
        UiPoint::Right => "RIGHT",
        UiPoint::BottomLeft => "BOTTOMLEFT",
        UiPoint::Bottom => "BOTTOM",
        UiPoint::BottomRight => "BOTTOMRIGHT",
    }
}

fn runtime_name(template: &str, name: Option<&str>) -> Option<UiRuntimeName> {
    let name = name?;
    if name == template {
        Some(UiRuntimeName::Root)
    } else if let Some(suffix) = name.strip_prefix(template) {
        Some(UiRuntimeName::RootSuffix(suffix.to_owned()))
    } else {
        Some(UiRuntimeName::Absolute(name.to_owned()))
    }
}

fn remap_target(
    functions: &mut Vec<RegistryKey>,
    scripts: &UiScriptPlan,
    lua: &Lua,
    target: &UiScriptTarget,
    template: &str,
) -> Result<UiScriptTarget, UiScriptError> {
    match target {
        UiScriptTarget::Global(name) => Ok(UiScriptTarget::Global(name.clone())),
        UiScriptTarget::Compiled(index) => {
            let function = scripts
                .compiled_function(lua, *index)
                .map_err(|error| template_error(template, error))?;
            let key = lua
                .create_registry_value(function)
                .map_err(|error| template_error(template, error))?;
            let index =
                u32::try_from(functions.len()).map_err(|error| template_error(template, error))?;
            functions.push(key);
            Ok(UiScriptTarget::Compiled(index))
        }
    }
}

fn template_error(template: &str, error: impl std::fmt::Display) -> UiScriptError {
    UiScriptError::Template {
        template: template.to_owned(),
        message: error.to_string(),
    }
}
