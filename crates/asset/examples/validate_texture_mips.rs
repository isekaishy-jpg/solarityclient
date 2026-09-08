//! Audits DXT mip sizes in installed archives without decoding their pixels.
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, BlpTextureSource, ClientDataRoot, Locale,
};
use std::{collections::BTreeSet, error::Error};
fn main() -> Result<(), Box<dyn Error>> {
    let root = std::env::args_os().nth(1).ok_or("data root required")?;
    let catalog = ArchiveCatalog::discover(ClientDataRoot::new(root)?, Locale::EnUs)?;
    let mut names = BTreeSet::new();
    if let Some(path) = std::env::args_os().nth(2) {
        names.extend(std::fs::read_to_string(path)?.lines().map(str::to_owned));
    } else {
        for archive in catalog.descriptors() {
            let mut mpq = wow_mpq::Archive::open(archive.path())?;
            if let Ok(list) = mpq.read_file("(listfile)") {
                for line in String::from_utf8_lossy(&list).lines() {
                    if line.to_ascii_lowercase().ends_with(".blp") {
                        names.insert(line.to_ascii_uppercase());
                    }
                }
            }
        }
    }
    let mut store = AssetStore::mount(catalog)?;
    println!("BLP files={}", names.len());
    let mut short = 0;
    for name in names {
        let path = AssetPath::new(&name)?;
        let source = BlpTextureSource::load(&mut store, &path)?;
        for mip in 0..source.mip_count() {
            if let Some(blocks) = source.block_mip(mip) {
                assert!(blocks.bytes().len() <= blocks.upload_byte_count());
            }
        }
        let read = store.read(&path)?;
        let b = read.bytes();
        if b.len() < 148 || &b[..8] != b"BLP2\x01\0\0\0" || b[8] != 2 {
            continue;
        }
        let word = |i| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let (w, h) = (word(12), word(16));
        let block = if b[10] == 0 { 8 } else { 16 };
        for mip in 0..16 {
            let offset = word(20 + mip * 4);
            let size = word(84 + mip * 4);
            if offset == 0 || size == 0 {
                break;
            }
            let (mw, mh) = ((w >> mip).max(1), (h >> mip).max(1));
            let expected = mw.div_ceil(4) * mh.div_ceil(4) * block;
            if size < expected {
                println!(
                    "path={name} mip={mip} dimensions={mw}x{mh} alpha={} type={} bytes={size} expected={expected}",
                    b[9], b[10]
                );
                short += 1;
            }
            if b[11] == 0 {
                break;
            }
        }
    }
    println!("short_mips={short}");
    Ok(())
}
