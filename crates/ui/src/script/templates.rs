//! Owned runtime prototypes for XML templates used by `CreateFrame`.

use mlua::{Lua, RegistryKey};

use crate::{
    FontCatalog, UiLayoutPlan, UiObjectCatalog, UiObjectKind, UiObjectTree, UiScriptError,
    UiScriptHandler, UiScriptPlan, UiScriptTarget,
};

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
    parent: Option<usize>,
    external_parent: Option<String>,
    construction_children: Vec<usize>,
    dimensions: (f64, f64),
    shown: bool,
    load_target: Option<UiScriptTarget>,
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

    pub(super) fn load_target(&self) -> Option<&UiScriptTarget> {
        self.load_target.as_ref()
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
            let first_node = plan.nodes.len();
            for (local_index, object) in tree.nodes().iter().enumerate() {
                let (dimensions, shown) =
                    resolve_local_region(&layout, local_index, template_name)?;
                let load_target = scripts
                    .binding(local_index, UiScriptHandler::Load)
                    .map(|binding| {
                        remap_target(
                            &mut plan.functions,
                            &scripts,
                            lua,
                            binding.target(),
                            template_name,
                        )
                    })
                    .transpose()?;
                plan.nodes.push(UiRuntimeTemplateNode {
                    name: runtime_name(template_name, object.name()),
                    kind: object.kind(),
                    parent: object.parent().map(|index| first_node + index),
                    external_parent: tree.pending_parent_name(local_index).map(str::to_owned),
                    construction_children: object
                        .construction_children()
                        .iter()
                        .map(|index| first_node + index)
                        .collect(),
                    dimensions,
                    shown,
                    load_target,
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
                if let Some(target) = node.load_target() {
                    match target {
                        UiScriptTarget::Compiled(index) => {
                            record.raw_set("on_load", self.compiled_function(lua, *index)?)?;
                        }
                        UiScriptTarget::Global(name) => {
                            record.raw_set("on_load_global", name.as_str())?;
                        }
                    }
                }
                nodes.raw_set(local_index + 1, record)?;
            }
            descriptor.raw_set("nodes", nodes)?;
            templates.raw_set(template.name.as_str(), descriptor)?;
        }
        lua.set_named_registry_value(TEMPLATE_REGISTRY, templates)
    }
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

fn resolve_local_region(
    layout: &UiLayoutPlan,
    node_index: usize,
    template: &str,
) -> Result<((f64, f64), bool), UiScriptError> {
    let node = layout
        .node(node_index)
        .ok_or_else(|| UiScriptError::Template {
            template: template.to_owned(),
            message: format!("object {node_index} has no layout-plan entry"),
        })?;
    let mut width = 0.0;
    let mut height = 0.0;
    let mut shown = true;
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
    }
    Ok(((width, height), shown))
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
