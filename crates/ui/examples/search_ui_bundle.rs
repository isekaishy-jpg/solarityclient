//! Searches one built-in UI bundle without extracting the client archives.

use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{UiBundle, UiManifestKind, UiResourceContent, XmlContent};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os();
    let _executable = arguments.next();
    let data_root = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(usage_error)?;
    let locale = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage_error)?
        .parse::<Locale>()?;
    let kind = match arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .as_deref()
    {
        Some("glue") => UiManifestKind::Glue,
        Some("frame") => UiManifestKind::Frame,
        _ => return Err(usage_error().into()),
    };
    let needle = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage_error)?;
    if arguments.next().is_some() {
        return Err(usage_error().into());
    }

    let root = ClientDataRoot::new(data_root)?;
    let catalog = ArchiveCatalog::discover(root, locale)?;
    let mut store = AssetStore::mount(catalog)?;
    let bundle = UiBundle::load(&mut store, kind)?;
    for resource in bundle.resources() {
        match resource.content() {
            UiResourceContent::Lua(source) => {
                for (line_index, line) in source.as_str().lines().enumerate() {
                    if line.contains(&needle) {
                        println!("{}:{}:{line}", resource.path(), line_index + 1);
                    }
                }
            }
            UiResourceContent::Xml(document) => {
                for element_index in 0..document.element_count() {
                    let Some(element) = document.element(element_index) else {
                        continue;
                    };
                    if element.name().contains(&needle) {
                        let attributes = element
                            .attributes()
                            .iter()
                            .map(|attribute| {
                                format!(" {}=\"{}\"", attribute.name(), attribute.value())
                            })
                            .collect::<String>();
                        println!(
                            "{}:#{element_index}:<{}{attributes}>",
                            resource.path(),
                            element.name()
                        );
                    }
                    for attribute in element.attributes() {
                        if attribute.value().contains(&needle) {
                            println!(
                                "{}:#{}:{}={}",
                                resource.path(),
                                element_index,
                                attribute.name(),
                                attribute.value()
                            );
                        }
                    }
                    for content in element.content() {
                        if let XmlContent::Text(text) = content
                            && text.contains(&needle)
                        {
                            println!("{}:#{}:{text}", resource.path(), element_index);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn usage_error() -> IoError {
    IoError::new(
        ErrorKind::InvalidInput,
        "usage: search_ui_bundle <Data> <locale> <glue|frame> <text>",
    )
}
