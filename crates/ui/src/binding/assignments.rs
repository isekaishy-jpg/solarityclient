//! Default and saved key assignment command streams.

use std::collections::HashMap;

use solarity_asset::{AssetPath, AssetStore};

use crate::binding::{
    UiBindingAssignmentError, UiBindingCatalog, UiBindingKey, UiModifiedClickChord,
};

/// Archive member containing build-12340's complete initial key map.
const DEFAULT_BINDINGS_PATH: &str = "WTF\\DefaultBindings.wtf";

/// Stock binding set selected by `LoadBindings` and `SaveBindings`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiBindingMode {
    /// Executable-authored defaults loaded from the archive stack.
    Default,
    /// Account-wide assignments.
    Account,
    /// Realm-character-specific assignments.
    Character,
}

impl UiBindingMode {
    /// Decodes the exact integer accepted by stock binding APIs.
    fn from_stock(value: &str) -> Option<Self> {
        match value {
            "0" => Some(Self::Default),
            "1" => Some(Self::Account),
            "2" => Some(Self::Character),
            _ => None,
        }
    }

    /// Returns the integer serialized by the stock assignment stream.
    #[must_use]
    pub const fn stock_value(self) -> u8 {
        match self {
            Self::Default => 0,
            Self::Account => 1,
            Self::Character => 2,
        }
    }
}

/// Action selected by one key chord.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiBindingAction {
    /// A named Lua binding declaration.
    Command(String),
    /// A spell resolved by its authored client name.
    Spell(String),
    /// An item resolved by its authored client name.
    Item(String),
    /// A macro resolved by its saved name.
    Macro(String),
    /// A named UI button and mouse button invoked by secure click routing.
    Click {
        /// Global button object name.
        button: String,
        /// Mouse-button name supplied to the click handler.
        mouse_button: String,
    },
}

impl UiBindingAction {
    /// Decodes stock's named and dynamic action prefixes.
    fn parse(value: &str) -> Result<Self, &'static str> {
        if let Some(value) = value.strip_prefix("SPELL ") {
            return Ok(Self::Spell(dynamic_value(value)?));
        }
        if let Some(value) = value.strip_prefix("ITEM ") {
            return Ok(Self::Item(dynamic_value(value)?));
        }
        if let Some(value) = value.strip_prefix("MACRO ") {
            return Ok(Self::Macro(dynamic_value(value)?));
        }
        if let Some(value) = value.strip_prefix("CLICK ") {
            let (button, mouse_button) = value
                .split_once(':')
                .ok_or("CLICK action has no mouse-button separator")?;
            if button.is_empty() || mouse_button.is_empty() || mouse_button.contains(':') {
                return Err("CLICK action has an invalid button identity");
            }
            validate_dynamic_text(button)?;
            validate_dynamic_text(mouse_button)?;
            return Ok(Self::Click {
                button: button.to_owned(),
                mouse_button: mouse_button.to_owned(),
            });
        }

        if value.is_empty() || value.bytes().any(|byte| byte.is_ascii_whitespace()) {
            return Err("named binding action is empty or contains whitespace");
        }
        if !value.is_ascii() || value.bytes().any(|byte| byte.is_ascii_lowercase()) {
            return Err("named binding action is not in stock uppercase ASCII form");
        }
        Ok(Self::Command(value.to_owned()))
    }
}

/// One effective key-to-action assignment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiBindingAssignment {
    key: UiBindingKey,
    action: UiBindingAction,
}

impl UiBindingAssignment {
    /// Returns the exact stock key token.
    #[must_use]
    pub const fn key(&self) -> &UiBindingKey {
        &self.key
    }

    /// Returns the action selected by this key.
    #[must_use]
    pub const fn action(&self) -> &UiBindingAction {
        &self.action
    }
}

/// One effective modified-click assignment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiModifiedClickAssignment {
    action: String,
    chord: UiModifiedClickChord,
}

impl UiModifiedClickAssignment {
    /// Returns the action identity declared by `Bindings.xml`.
    #[must_use]
    pub fn action(&self) -> &str {
        &self.action
    }

    /// Returns the selected modifier/button chord or `NONE`.
    #[must_use]
    pub const fn chord(&self) -> &UiModifiedClickChord {
        &self.chord
    }
}

/// One stock binding mode after sequential command application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiBindingAssignments {
    mode: UiBindingMode,
    bindings: Vec<UiBindingAssignment>,
    binding_indices: HashMap<UiBindingKey, usize>,
    modified_clicks: Vec<UiModifiedClickAssignment>,
    modified_click_indices: HashMap<String, usize>,
}

impl UiBindingAssignments {
    /// Loads the full executable-authored default key map through MPQ precedence.
    ///
    /// Modified-click defaults come from the already validated binding catalog,
    /// matching their ownership in `Bindings.xml` rather than this WTF member.
    ///
    /// # Errors
    ///
    /// Returns an asset, UTF-8, or assignment-record error.
    pub fn load_defaults(
        store: &mut AssetStore,
        catalog: &UiBindingCatalog,
    ) -> Result<Self, UiBindingAssignmentError> {
        let path = AssetPath::new(DEFAULT_BINDINGS_PATH)?;
        let bytes = store.read(&path)?.into_bytes();
        let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
        let source =
            std::str::from_utf8(bytes).map_err(|error| UiBindingAssignmentError::TextEncoding {
                path,
                message: error.to_string(),
            })?;
        Self::parse(source, UiBindingMode::Default, catalog)
    }

    /// Applies a saved account or character assignment command stream.
    ///
    /// Duplicate key or modified-click records execute sequentially, so the
    /// last record replaces the earlier value exactly as stock's `bind` and
    /// `modifiedclick` commands do.
    ///
    /// # Errors
    ///
    /// Returns the first invalid command, mode, key, action, or click record.
    pub fn parse(
        source: &str,
        mode: UiBindingMode,
        catalog: &UiBindingCatalog,
    ) -> Result<Self, UiBindingAssignmentError> {
        let source = source.strip_prefix('\u{feff}').unwrap_or(source);
        let mut assignments = Self::with_modified_click_defaults(mode, catalog);
        let mut declared_mode = None;
        for (index, source_line) in source.lines().enumerate() {
            let line_number = index + 1;
            let line = source_line.trim().trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            let (command, remainder) = take_token(line)
                .ok_or_else(|| record_error(line_number, "assignment record has no arguments"))?;
            match command {
                "bind" => assignments.apply_bind(line_number, remainder)?,
                "modifiedclick" => assignments.apply_modified_click(line_number, remainder)?,
                "BINDINGMODE" => {
                    if declared_mode.is_some() {
                        return Err(record_error(line_number, "BINDINGMODE is repeated"));
                    }
                    let value = one_token(line_number, remainder, "BINDINGMODE")?;
                    let parsed = UiBindingMode::from_stock(value).ok_or_else(|| {
                        record_error(line_number, "BINDINGMODE is not 0, 1, or 2")
                    })?;
                    if parsed != mode {
                        return Err(record_error(
                            line_number,
                            format!(
                                "BINDINGMODE {} does not match requested mode {}",
                                parsed.stock_value(),
                                mode.stock_value()
                            ),
                        ));
                    }
                    declared_mode = Some(parsed);
                }
                unknown => {
                    return Err(record_error(
                        line_number,
                        format!("unknown assignment command {unknown}"),
                    ));
                }
            }
        }
        Ok(assignments)
    }

    /// Returns the binding set represented by this state.
    #[must_use]
    pub const fn mode(&self) -> UiBindingMode {
        self.mode
    }

    /// Returns effective key assignments in first-key appearance order.
    #[must_use]
    pub fn bindings(&self) -> &[UiBindingAssignment] {
        &self.bindings
    }

    /// Looks up one exact serialized key without allocating a wrapper.
    #[must_use]
    pub fn binding_for(&self, key: &str) -> Option<&UiBindingAssignment> {
        self.binding_indices
            .get(key)
            .map(|index| &self.bindings[*index])
    }

    /// Returns effective modified-click assignments in declaration order.
    #[must_use]
    pub fn modified_clicks(&self) -> &[UiModifiedClickAssignment] {
        &self.modified_clicks
    }

    /// Looks up a modified-click action declared by `Bindings.xml`.
    #[must_use]
    pub fn modified_click(&self, action: &str) -> Option<&UiModifiedClickAssignment> {
        self.modified_click_indices
            .get(action)
            .map(|index| &self.modified_clicks[*index])
    }

    /// Seeds the exact defaults owned by the binding declaration catalog.
    fn with_modified_click_defaults(mode: UiBindingMode, catalog: &UiBindingCatalog) -> Self {
        let mut assignments = Self {
            mode,
            bindings: Vec::new(),
            binding_indices: HashMap::new(),
            modified_clicks: Vec::new(),
            modified_click_indices: HashMap::new(),
        };
        for definition in catalog.modified_clicks() {
            let index = assignments.modified_clicks.len();
            assignments
                .modified_click_indices
                .insert(definition.action().to_owned(), index);
            assignments.modified_clicks.push(UiModifiedClickAssignment {
                action: definition.action().to_owned(),
                chord: definition.default_chord().clone(),
            });
        }
        assignments
    }

    /// Applies one `bind KEY ACTION` record with stock last-write behavior.
    fn apply_bind(&mut self, line: usize, remainder: &str) -> Result<(), UiBindingAssignmentError> {
        let (key, action) =
            take_token(remainder).ok_or_else(|| record_error(line, "bind record has no action"))?;
        if action.is_empty() {
            return Err(record_error(line, "bind record has no action"));
        }
        let key = UiBindingKey::parse(key).map_err(|message| record_error(line, message))?;
        let action =
            UiBindingAction::parse(action).map_err(|message| record_error(line, message))?;
        if let Some(index) = self.binding_indices.get(key.as_str()).copied() {
            self.bindings[index].action = action;
            return Ok(());
        }
        let index = self.bindings.len();
        self.binding_indices.insert(key.clone(), index);
        self.bindings.push(UiBindingAssignment { key, action });
        Ok(())
    }

    /// Applies one `modifiedclick ACTION CHORD` record to a declared action.
    fn apply_modified_click(
        &mut self,
        line: usize,
        remainder: &str,
    ) -> Result<(), UiBindingAssignmentError> {
        let (action, remainder) = take_token(remainder)
            .ok_or_else(|| record_error(line, "modifiedclick record has no chord"))?;
        let chord = one_token(line, remainder, "modifiedclick")?;
        let chord =
            UiModifiedClickChord::parse(chord).map_err(|message| record_error(line, message))?;
        let Some(index) = self.modified_click_indices.get(action).copied() else {
            return Err(record_error(
                line,
                format!("modifiedclick action {action} was not declared"),
            ));
        };
        self.modified_clicks[index].chord = chord;
        Ok(())
    }
}

/// Splits the first whitespace-delimited token while retaining the rest.
fn take_token(value: &str) -> Option<(&str, &str)> {
    let boundary = value.find(char::is_whitespace)?;
    let token = &value[..boundary];
    let remainder = value[boundary..].trim();
    (!token.is_empty()).then_some((token, remainder))
}

/// Requires exactly one remaining token for fixed-arity records.
fn one_token<'a>(
    line: usize,
    value: &'a str,
    command: &str,
) -> Result<&'a str, UiBindingAssignmentError> {
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return Err(record_error(
            line,
            format!("{command} record does not have exactly one argument"),
        ));
    }
    Ok(value)
}

/// Validates a dynamic spell, item, macro, or click string.
fn dynamic_value(value: &str) -> Result<String, &'static str> {
    validate_dynamic_text(value)?;
    Ok(value.to_owned())
}

/// Rejects empty, padded, or control-bearing dynamic action payloads.
fn validate_dynamic_text(value: &str) -> Result<(), &'static str> {
    if value.is_empty() || value.trim() != value {
        return Err("dynamic binding action is empty or padded");
    }
    if value.chars().any(char::is_control) {
        return Err("dynamic binding action contains a control character");
    }
    Ok(())
}

/// Constructs one stable line-oriented assignment failure.
fn record_error(line: usize, message: impl Into<String>) -> UiBindingAssignmentError {
    UiBindingAssignmentError::Record {
        line,
        message: message.into(),
    }
}
