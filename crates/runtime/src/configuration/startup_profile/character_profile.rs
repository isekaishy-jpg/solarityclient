//! Local character CVar cache owned by native 6B9050/6B9BE0, data type one.

use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use super::config_wtf::{parse_cvar_values, replace_cvar_values};
use crate::configuration::ConfigurationError;

/// Preserves unrelated cache records while the implemented character CVars change.
#[derive(Debug)]
pub(crate) struct CharacterProfile {
    path: PathBuf,
    contents: String,
    values: Vec<(String, String)>,
}

impl CharacterProfile {
    /// Native 6B8B90 selects Account/account/realm/character/config-cache.wtf.
    pub(super) fn load(
        account_root: PathBuf,
        account: &str,
        realm: &str,
        character: &str,
    ) -> Result<Self, ConfigurationError> {
        let mut path = account_root;
        for name in [account, realm, character] {
            let mut components = Path::new(name).components();
            if !matches!(components.next(), Some(Component::Normal(_)))
                || components.next().is_some()
                || name.contains(['/', '\\', ':'])
            {
                return Err(ConfigurationError::ProfileRead {
                    path,
                    message: format!("invalid character profile component {name:?}"),
                });
            }
            path.push(name);
        }
        path.push("config-cache.wtf");
        let contents = match fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
            Err(error) => {
                return Err(ConfigurationError::ProfileRead {
                    path,
                    message: error.to_string(),
                });
            }
        };
        let values = parse_cvar_values(&contents);
        Ok(Self {
            path,
            contents,
            values,
        })
    }

    pub(crate) fn cvar_values(&self) -> &[(String, String)] {
        &self.values
    }

    /// Reads finite numeric state; absent declarations retain the registered default.
    pub(crate) fn number(&self, name: &str, default: f32) -> Result<f32, ConfigurationError> {
        let Some((_, value)) = self
            .values
            .iter()
            .rev()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
        else {
            return Ok(default);
        };
        value
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
            .ok_or_else(|| ConfigurationError::ProfileRead {
                path: self.path.clone(),
                message: format!("CVar {name} value {value:?} is not a finite number"),
            })
    }

    /// Rewrites only the specified declarations, retaining other character settings.
    pub(crate) fn persist_cvars(
        &mut self,
        values: &[(String, String)],
    ) -> Result<(), ConfigurationError> {
        if values.is_empty() {
            return Ok(());
        }
        for (name, value) in values {
            if name.is_empty()
                || name.chars().any(char::is_whitespace)
                || value.contains(['"', '\r', '\n'])
            {
                return Err(ConfigurationError::ProfileWrite {
                    path: self.path.clone(),
                    message: format!("CVar {name:?} cannot be represented in config-cache.wtf"),
                });
            }
        }
        let replacement = replace_cvar_values(&self.contents, values);
        let parent = self
            .path
            .parent()
            .ok_or_else(|| ConfigurationError::ProfileWrite {
                path: self.path.clone(),
                message: "character profile has no parent directory".to_owned(),
            })?;
        fs::create_dir_all(parent)
            .and_then(|()| fs::write(&self.path, &replacement))
            .map_err(|error| ConfigurationError::ProfileWrite {
                path: self.path.clone(),
                message: error.to_string(),
            })?;
        self.contents = replacement;
        self.values = parse_cvar_values(&self.contents);
        Ok(())
    }
}
