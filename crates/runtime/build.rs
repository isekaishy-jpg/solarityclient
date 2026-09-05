//! Embeds the product version, reserved package number, and source revision.

#[path = "build_support/version.rs"]
mod version;

use std::env;
use std::error::Error;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

/// Records immutable artifact identity; reserving package numbers belongs to
/// build-client.ps1, so ordinary Cargo checks and rebuilds never increment it.
fn main() -> Result<(), Box<dyn Error>> {
    let manifest = env::var("CARGO_MANIFEST_DIR")?;
    let root = Path::new(&manifest).join("../..").canonicalize()?;
    let product = version::product_version(&env::var("CARGO_PKG_VERSION")?)?;
    let number = fs::read_to_string(root.join("BUILD_NUMBER"))?
        .trim()
        .parse::<u32>()?;
    let revision = git(&root, &["rev-parse", "--verify", "HEAD"])?;
    let dirty = !git(
        &root,
        &["status", "--porcelain", "--untracked-files=normal"],
    )?
    .is_empty();
    for path in [
        "Cargo.toml",
        "Cargo.lock",
        "BUILD_NUMBER",
        "STYLE.md",
        "crates",
        "scripts",
        "docs",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(path).display());
    }
    // Worktrees keep HEAD/index separately while sharing branch references.
    for path in ["HEAD", "index", "refs", "packed-refs"] {
        let git_path = git(&root, &["rev-parse", "--git-path", path])?;
        println!("cargo:rerun-if-changed={}", root.join(git_path).display());
    }
    let output = Path::new(&env::var("OUT_DIR")?).join("client_build.rs");
    fs::write(
        output,
        format!(
            "/// Identity captured when this executable was compiled.\npub const CLIENT_BUILD: ClientBuild = ClientBuild::new({product:?}, {number}, {revision:?}, {dirty});\n"
        ),
    )?;
    Ok(())
}

/// Reads source identity from the same checkout that Cargo is compiling.
fn git(root: &Path, arguments: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = Command::new("git")
        .current_dir(root)
        .args(arguments)
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "cannot read build source identity: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}
