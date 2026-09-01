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
        Ok(Self {
            config_path,
            contents,
            play_intro_movie,
        })
    }

    /// Returns whether native startup must consume the intro-movie request.
    #[must_use]
    pub const fn play_intro_movie(&self) -> bool {
        self.play_intro_movie
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
        self.play_intro_movie = false;
        Ok(true)
    }
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

/// Parses the exact three-token `SET name "value"` Config.wtf record shape.
fn set_value<'line>(line: &'line str, requested_name: &str) -> Option<&'line str> {
    let mut fields = line.split_whitespace();
    let command = fields.next()?;
    let name = fields.next()?;
    let quoted = fields.next()?;
    if fields.next().is_some()
        || !command.eq_ignore_ascii_case("SET")
        || !name.eq_ignore_ascii_case(requested_name)
        || quoted.len() < 2
        || !quoted.starts_with('"')
        || !quoted.ends_with('"')
    {
        return None;
    }
    quoted.get(1..quoted.len() - 1)
}
