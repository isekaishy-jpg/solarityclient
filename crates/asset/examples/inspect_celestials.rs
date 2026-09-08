//! Validate native celestial BLP requests through installed archive precedence.
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, BlpTextureSource, ClientDataRoot, Locale,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args_os()
        .nth(1)
        .ok_or("expected Data directory")?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    for path in [
        "Textures/sunCenter.blp",
        "Textures/moon.blp",
        "Textures/moon02.blp",
    ] {
        let source = BlpTextureSource::load(&mut store, &AssetPath::new(path)?)?;
        let decoded = source.decode_mip(0)?;
        let visible = decoded
            .rgba8()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] != 0)
            .count();
        println!(
            "{}: {}x{}, {} mips, {visible} visible texels, {:?}",
            source.path(),
            source.width(),
            source.height(),
            source.mip_count(),
            source.source()
        );
    }
    Ok(())
}
