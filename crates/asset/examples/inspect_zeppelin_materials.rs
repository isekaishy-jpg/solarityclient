//! Inspects exact transport material and doodad inputs without a world session.

use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedM2Model, DecodedWorldModel,
    Locale,
};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let root = std::env::args().nth(1).ok_or("missing Data root")?;
    let catalog = ArchiveCatalog::discover(ClientDataRoot::new(root)?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    for name in ["TRANSPORT_HORDE_ZEPPELIN", "TRANSPORT_ZEPPELIN"] {
        let path = AssetPath::new(format!("World/Wmo/Transports/{name}/{name}.wmo"))?;
        let model = DecodedWorldModel::load(&mut store, &path)?;
        println!(
            "{path}: flags={} materials={} doodads={}",
            model.flags(),
            model.materials().len(),
            model.doodads().len()
        );
        for (index, doodad) in model.doodads().iter().enumerate() {
            if doodad.path().as_str().contains("LIGHT")
                || doodad.path().as_str().contains("ANIMATION")
            {
                let m2 = DecodedM2Model::load_primary_profile(&mut store, doodad.path())?;
                println!(
                    "doodad {index}: path={} colors={:?}",
                    doodad.path(),
                    m2.animations().colors()
                );
                for skin in m2.skins() {
                    for batch in skin
                        .batches()
                        .iter()
                        .filter(|batch| batch.color_index != u16::MAX)
                    {
                        let texture = m2
                            .texture_lookup()
                            .get(usize::from(batch.texture_combo_index))
                            .and_then(|&index| m2.textures().get(usize::from(index)));
                        println!(
                            "  colored batch={batch:?} material={:?} texture={texture:?}",
                            m2.materials().get(usize::from(batch.material_index))
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
