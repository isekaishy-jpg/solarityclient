//! Validated tokens used by stock key and modified-click assignments.

use std::borrow::Borrow;

/// One exact key chord emitted by the stock binding serializer.
///
/// The token retains stock spellings such as `CTRL-SHIFT-PAGEDOWN`, `/`, and
/// `CTRL--`. Decomposing on hyphens would make the minus key ambiguous, so
/// physical-event translation remains a separate, evidence-backed boundary.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct UiBindingKey(String);

impl UiBindingKey {
    /// Validates one generated key token without changing its identity.
    pub(super) fn parse(value: &str) -> Result<Self, &'static str> {
        validate_token(value)?;
        if value == "NONE" {
            return Err("NONE is not a bindable key");
        }
        Ok(Self(value.to_owned()))
    }

    /// Returns the exact serialized key chord.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for UiBindingKey {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

/// One exact modified-click chord or the stock `NONE` sentinel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiModifiedClickChord(String);

impl UiModifiedClickChord {
    /// Validates the modifier/button vocabulary used by build 12340.
    pub(super) fn parse(value: &str) -> Result<Self, &'static str> {
        validate_token(value)?;
        if value == "NONE" {
            return Ok(Self(value.to_owned()));
        }

        let mut alt = false;
        let mut control = false;
        let mut shift = false;
        let mut button = false;
        for component in value.split('-') {
            match component {
                "ALT" if !alt && !button => alt = true,
                "CTRL" if !control && !button => control = true,
                "SHIFT" if !shift && !button => shift = true,
                "BUTTON1" | "BUTTON2" | "BUTTON3" | "BUTTON4" | "BUTTON5" if !button => {
                    button = true;
                }
                _ => return Err("invalid or repeated modified-click component"),
            }
        }
        if !alt && !control && !shift && !button {
            return Err("modified-click chord has no component");
        }
        Ok(Self(value.to_owned()))
    }

    /// Returns the exact serialized modified-click chord.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Reports whether the action is explicitly disabled.
    #[must_use]
    pub fn is_none(&self) -> bool {
        self.0 == "NONE"
    }
}

/// Requires one uppercase printable token with no embedded whitespace.
fn validate_token(value: &str) -> Result<(), &'static str> {
    if value.is_empty() {
        return Err("token is empty");
    }
    if !value.is_ascii() || value.bytes().any(|byte| !(0x21..=0x7E).contains(&byte)) {
        return Err("token is not printable ASCII without whitespace");
    }
    if value.bytes().any(|byte| byte.is_ascii_lowercase()) {
        return Err("token is not in stock uppercase form");
    }
    Ok(())
}
