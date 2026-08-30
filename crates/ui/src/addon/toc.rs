//! Build-12340 TOC metadata parsing independent of storage.

use std::collections::BTreeMap;

use solarity_asset::Locale;

use super::{AddonCatalogError, AddonCompatibility, AddonDefinition, STOCK_INTERFACE_VERSION};

/// Parsed values which still need storage-derived signature state.
pub(super) struct ParsedToc {
    metadata: BTreeMap<String, String>,
    entrypoints: Vec<String>,
}

impl ParsedToc {
    pub(super) fn parse(addon: &str, bytes: &[u8]) -> Result<Self, AddonCatalogError> {
        let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
        let source =
            std::str::from_utf8(bytes).map_err(|error| AddonCatalogError::InvalidEncoding {
                addon: addon.to_owned(),
                message: error.to_string(),
            })?;
        let mut metadata = BTreeMap::new();
        let mut entrypoints = Vec::new();
        for source_line in source.lines() {
            let line = source_line.trim().trim_end_matches('\r');
            if line.is_empty() || (line.starts_with('#') && !line.starts_with("##")) {
                continue;
            }
            if let Some(directive) = line.strip_prefix("##") {
                if let Some((key, value)) = directive.split_once(':') {
                    metadata.insert(key.trim().to_ascii_lowercase(), value.trim().to_owned());
                }
                continue;
            }
            entrypoints.push(line.replace('/', "\\"));
        }
        Ok(Self {
            metadata,
            entrypoints,
        })
    }

    pub(super) fn finish(
        self,
        addon: String,
        locale: Locale,
        signed: bool,
    ) -> Result<AddonDefinition, AddonCatalogError> {
        let interface = self
            .metadata("interface")
            .map(|value| {
                value
                    .parse::<u32>()
                    .map_err(|_| AddonCatalogError::InvalidMetadata {
                        addon: addon.clone(),
                        field: "Interface",
                        value: value.to_owned(),
                    })
            })
            .transpose()?;
        let compatibility = match interface {
            Some(STOCK_INTERFACE_VERSION) => AddonCompatibility::Current,
            Some(version) if version < STOCK_INTERFACE_VERSION => AddonCompatibility::OutOfDate,
            Some(_) => AddonCompatibility::Newer,
            None => AddonCompatibility::MissingInterface,
        };
        let title = self
            .localized_metadata("title", locale)
            .unwrap_or(&addon)
            .to_owned();
        let notes = self.localized_metadata("notes", locale).map(str::to_owned);
        let dependencies = self
            .metadata("dependencies")
            .or_else(|| self.metadata("requireddeps"))
            .map_or_else(Vec::new, parse_name_list);
        let optional_dependencies = self
            .metadata("optionaldeps")
            .map_or_else(Vec::new, parse_name_list);
        let load_on_demand = self.parse_bool(&addon, "LoadOnDemand", false)?;
        let enabled_by_default = self.parse_default_state(&addon)?;
        let secure = self.parse_bool(&addon, "Secure", false)?;
        let saved_variables = self
            .metadata("savedvariables")
            .map_or_else(Vec::new, parse_name_list);
        let saved_variables_per_character = self
            .metadata("savedvariablespercharacter")
            .map_or_else(Vec::new, parse_name_list);

        Ok(AddonDefinition {
            name: addon,
            title,
            notes,
            interface,
            compatibility,
            dependencies,
            optional_dependencies,
            load_on_demand,
            enabled_by_default,
            signed,
            secure,
            saved_variables,
            saved_variables_per_character,
            entrypoints: self.entrypoints,
        })
    }

    fn metadata(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(String::as_str)
    }

    fn localized_metadata(&self, key: &str, locale: Locale) -> Option<&str> {
        let localized_key = format!("{key}-{}", locale.as_str().to_ascii_lowercase());
        self.metadata(&localized_key).or_else(|| self.metadata(key))
    }

    fn parse_bool(
        &self,
        addon: &str,
        field: &'static str,
        absent: bool,
    ) -> Result<bool, AddonCatalogError> {
        let Some(value) = self.metadata(&field.to_ascii_lowercase()) else {
            return Ok(absent);
        };
        match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "enabled" => Ok(true),
            "0" | "false" | "disabled" => Ok(false),
            _ => Err(AddonCatalogError::InvalidMetadata {
                addon: addon.to_owned(),
                field,
                value: value.to_owned(),
            }),
        }
    }

    fn parse_default_state(&self, addon: &str) -> Result<bool, AddonCatalogError> {
        let Some(value) = self.metadata("defaultstate") else {
            return Ok(true);
        };
        match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "enabled" => Ok(true),
            "0" | "false" | "disabled" => Ok(false),
            _ => Err(AddonCatalogError::InvalidMetadata {
                addon: addon.to_owned(),
                field: "DefaultState",
                value: value.to_owned(),
            }),
        }
    }
}

fn parse_name_list(value: &str) -> Vec<String> {
    value
        .split([',', ' ', '\t'])
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}
