//! Narrow typed ownership of startup CVars needed before Glue exists.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::configuration::ConfigurationError;

const CONFIG_RELATIVE_PATH: &str = "WTF\\Config.wtf";
const PLAY_INTRO_MOVIE: &str = "playIntroMovie";

/// Persistent native startup state loaded before the Glue screen is selected.
#[derive(Debug)]
pub struct StartupProfile {
    config_path: PathBuf,
    contents: String,
    cvar_values: Vec<(String, String)>,
    play_intro_movie: bool,
}

impl StartupProfile {
    /// Loads `WTF\\Config.wtf` below one explicit client profile root.
    ///
    /// An absent file has the registered build-12340 default of one. Malformed
    /// declarations and filesystem failures remain explicit.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationError`] when the file cannot be read as UTF-8 or
    /// a present `playIntroMovie` declaration is not a signed integer.
    pub fn load(profile_root: impl AsRef<Path>) -> Result<Self, ConfigurationError> {
        let config_path = profile_root.as_ref().join(CONFIG_RELATIVE_PATH);
        let contents = match fs::read_to_string(&config_path) {
            Ok(contents) => contents,
            Err(source) if source.kind() == ErrorKind::NotFound => String::new(),
            Err(source) => {
                return Err(ConfigurationError::ProfileRead {
                    path: config_path,
                    message: source.to_string(),
                });
            }
        };
        let play_intro_movie = parse_play_intro_movie(&config_path, &contents)?.unwrap_or(true);
        let cvar_values = parse_cvar_values(&contents);
        Ok(Self {
            config_path,
            contents,
            cvar_values,
            play_intro_movie,
        })
    }

    /// Returns whether native startup must consume the intro-movie request.
    #[must_use]
    pub const fn play_intro_movie(&self) -> bool {
        self.play_intro_movie
    }

    /// Returns Config.wtf declarations in file order for native CVar loading.
    #[must_use]
    pub fn cvar_values(&self) -> &[(String, String)] {
        &self.cvar_values
    }

    /// Atomically consumes a pending intro request before selecting MovieFrame.
    ///
    /// The stock executable sets `playIntroMovie` to zero immediately before
    /// selecting the movie Glue screen. A false result performs no write.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationError`] when the profile directory or replacement
    /// file cannot be written.
    pub fn consume_intro_movie(&mut self) -> Result<bool, ConfigurationError> {
        if !self.play_intro_movie {
            return Ok(false);
        }
        let replacement = replace_play_intro_movie(&self.contents);
        let parent = self
            .config_path
            .parent()
            .ok_or_else(|| ConfigurationError::ProfileWrite {
                path: self.config_path.clone(),
                message: "Config.wtf has no parent directory".to_owned(),
            })?;
        fs::create_dir_all(parent).map_err(|source| ConfigurationError::ProfileWrite {
            path: parent.to_path_buf(),
            message: source.to_string(),
        })?;
        fs::write(&self.config_path, replacement.as_bytes()).map_err(|source| {
            ConfigurationError::ProfileWrite {
                path: self.config_path.clone(),
                message: source.to_string(),
            }
        })?;
        self.contents = replacement;
        self.cvar_values = parse_cvar_values(&self.contents);
        self.play_intro_movie = false;
        Ok(true)
    }

    /// Persists script-mutated CVars in one profile rewrite.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigurationError`] for values that cannot be represented by
    /// the stock declaration shape or when the profile cannot be written.
    pub fn persist_cvars(&mut self, values: &[(String, String)]) -> Result<(), ConfigurationError> {
        if values.is_empty() {
            return Ok(());
        }
        for (name, value) in values {
            if name.is_empty()
                || name.chars().any(char::is_whitespace)
                || value.contains(['"', '\r', '\n'])
            {
                return Err(ConfigurationError::ProfileWrite {
                    path: self.config_path.clone(),
                    message: format!("CVar {name:?} cannot be represented in Config.wtf"),
                });
            }
        }
        let replacement = replace_cvar_values(&self.contents, values);
        let parent = self
            .config_path
            .parent()
            .ok_or_else(|| ConfigurationError::ProfileWrite {
                path: self.config_path.clone(),
                message: "Config.wtf has no parent directory".to_owned(),
            })?;
        fs::create_dir_all(parent).map_err(|source| ConfigurationError::ProfileWrite {
            path: parent.to_path_buf(),
            message: source.to_string(),
        })?;
        fs::write(&self.config_path, replacement.as_bytes()).map_err(|source| {
            ConfigurationError::ProfileWrite {
                path: self.config_path.clone(),
                message: source.to_string(),
            }
        })?;
        self.contents = replacement;
        self.cvar_values = parse_cvar_values(&self.contents);
        self.play_intro_movie =
            parse_play_intro_movie(&self.config_path, &self.contents)?.unwrap_or(true);
        Ok(())
    }
}

fn parse_cvar_values(contents: &str) -> Vec<(String, String)> {
    contents
        .lines()
        .filter_map(set_record)
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect()
}

/// Applies sequential CVar-load semantics; the last declaration wins.
fn parse_play_intro_movie(path: &Path, contents: &str) -> Result<Option<bool>, ConfigurationError> {
    let mut result = None;
    for (line_index, line) in contents.lines().enumerate() {
        let Some(value) = set_value(line, PLAY_INTRO_MOVIE) else {
            continue;
        };
        let value = value
            .parse::<i32>()
            .map_err(|_source| ConfigurationError::ProfileParse {
                path: path.to_path_buf(),
                line: line_index + 1,
                message: format!("{PLAY_INTRO_MOVIE} value {value:?} is not a signed integer"),
            })?;
        result = Some(value != 0);
    }
    Ok(result)
}

/// Rewrites every existing declaration and appends one when it was absent.
fn replace_play_intro_movie(contents: &str) -> String {
    let mut found = false;
    let mut lines = contents
        .lines()
        .map(|line| {
            if set_value(line, PLAY_INTRO_MOVIE).is_some() {
                found = true;
                format!("SET {PLAY_INTRO_MOVIE} \"0\"")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>();
    if !found {
        lines.push(format!("SET {PLAY_INTRO_MOVIE} \"0\""));
    }
    let mut output = lines.join("\r\n");
    output.push_str("\r\n");
    output
}

fn replace_cvar_values(contents: &str, values: &[(String, String)]) -> String {
    let mut found = vec![false; values.len()];
    let mut lines = contents
        .lines()
        .map(|line| {
            let Some((name, _value)) = set_record(line) else {
                return line.to_owned();
            };
            let Some((index, (requested_name, requested_value))) =
                values
                    .iter()
                    .enumerate()
                    .find(|(_index, (requested_name, _value))| {
                        name.eq_ignore_ascii_case(requested_name)
                    })
            else {
                return line.to_owned();
            };
            found[index] = true;
            format!("SET {requested_name} \"{requested_value}\"")
        })
        .collect::<Vec<_>>();
    for ((name, value), found) in values.iter().zip(found) {
        if !found {
            lines.push(format!("SET {name} \"{value}\""));
        }
    }
    let mut output = lines.join("\r\n");
    output.push_str("\r\n");
    output
}

/// Parses the exact three-token `SET name "value"` Config.wtf record shape.
fn set_value<'line>(line: &'line str, requested_name: &str) -> Option<&'line str> {
    let (name, value) = set_record(line)?;
    name.eq_ignore_ascii_case(requested_name).then_some(value)
}

fn set_record(line: &str) -> Option<(&str, &str)> {
    let mut fields = line.split_whitespace();
    let command = fields.next()?;
    let name = fields.next()?;
    let quoted = fields.next()?;
    if fields.next().is_some()
        || !command.eq_ignore_ascii_case("SET")
        || quoted.len() < 2
        || !quoted.starts_with('"')
        || !quoted.ends_with('"')
    {
        return None;
    }
    Some((name, quoted.get(1..quoted.len() - 1)?))
}
