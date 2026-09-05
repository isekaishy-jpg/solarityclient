//! Validation of the repository's product identity at compile time.

/// Converts Cargo's SemVer spelling into the user-facing release-stage suffix.
pub fn product_version(cargo_version: &str) -> Result<String, &'static str> {
    let (base, suffix) = match cargo_version.split_once('-') {
        Some((base, "alpha")) => (base, "a"),
        Some((base, "beta")) => (base, "b"),
        Some((base, "rc")) => (base, "rc"),
        Some(_) => return Err("unsupported product release stage"),
        None => (cargo_version, "s"),
    };
    let components: Vec<_> = base.split('.').collect();
    if components.len() != 3
        || components.iter().any(|component| {
            component.is_empty()
                || !component.bytes().all(|byte| byte.is_ascii_digit())
                || (component.len() > 1 && component.starts_with('0'))
        })
    {
        return Err("product version must have three numeric components");
    }
    Ok(format!("{base}{suffix}"))
}
