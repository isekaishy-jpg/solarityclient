//! CEffect's exact named row selection and authored model dependencies.

use std::error::Error;

use solarity_asset::{
    ArchiveCatalog, AssetStore, ClientDataRoot, DecodedM2Model, Locale, SpellVisualEffectCatalog,
    canonical_model_path,
};

use crate::support::{Fixture, FixtureFile};

#[test]
fn spell_visual_effect_names_preserve_case_and_physical_row_order() -> Result<(), Box<dyn Error>> {
    let table = fixture_table(&[
        (200, "HARDCODED Breath Underwater", "Particles/Bubbles.mdl"),
        (
            2,
            "HARDCODED Breath Underwater",
            "Particles/Replacement.mdx",
        ),
        (4, "hardcoded breath underwater", ""),
        (5, "Unused export", "\\\\export-server\\project\\effect.mdx"),
    ]);
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient/SpellVisualEffectName.dbc",
        bytes: &table,
    }])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let catalog = SpellVisualEffectCatalog::load(&mut store)?;
    let effect = catalog
        .named("HARDCODED Breath Underwater")
        .ok_or("effect")?;
    assert_eq!(effect.id(), 2);
    assert_eq!(effect.name(), "HARDCODED Breath Underwater");
    assert_eq!(
        canonical_model_path(&effect.model_path()?.ok_or("model")?)?.as_str(),
        "PARTICLES\\REPLACEMENT.M2"
    );
    assert_eq!(effect.area_effect_size(), 9.0);
    assert_eq!(effect.scale(), 2.0);
    assert_eq!(effect.min_scale(), 0.25);
    assert_eq!(effect.max_scale(), 4.0);
    assert_eq!(catalog.definition(200).ok_or("original")?.id(), 200);
    assert!(catalog.named("HARDCODED BREATH UNDERWATER").is_none());
    assert!(
        catalog
            .named("hardcoded breath underwater")
            .ok_or("lowercase")?
            .model_path()?
            .is_none()
    );
    assert!(catalog.definition(5).ok_or("unused")?.model_path().is_err());
    assert!(catalog.definition(108).is_none());
    Ok(())
}

#[test]
fn spell_visual_effect_rejects_duplicate_keys_and_invalid_strings() -> Result<(), Box<dyn Error>> {
    let valid = fixture_table(&[(8, "first", ""), (8, "second", "")]);
    let mut invalid_offset = fixture_table(&[(8, "first", "")]);
    invalid_offset[24..28].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut wrong_layout = fixture_table(&[(8, "first", "")]);
    wrong_layout[8..12].copy_from_slice(&6_u32.to_le_bytes());
    for bytes in [&valid, &invalid_offset, &wrong_layout] {
        let fixture = Fixture::new(&[FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient/SpellVisualEffectName.dbc",
            bytes,
        }])?;
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        assert!(SpellVisualEffectCatalog::load(&mut store).is_err());
    }
    Ok(())
}

#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with locally owned build-12340 archives"]
fn spell_visual_effect_stock_water_models_decode() -> Result<(), Box<dyn Error>> {
    let root = std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("stock data root")?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    let catalog = SpellVisualEffectCatalog::load(&mut store)?;
    for (name, id, model_name, duration) in [
        ("Breath Underwater", 108, "PARTICLES\\BUBBLES.M2", 3333),
        ("Breath Cold", 107, "PARTICLES\\COLDBREATH.M2", 1500),
        (
            "Footstep Water Run Spray",
            200,
            "PARTICLES\\FOOTSTEPSPRAYWATER.M2",
            667,
        ),
        (
            "Footstep Water Walk Spray",
            201,
            "PARTICLES\\FOOTSTEPSPRAYWATERWALK.M2",
            667,
        ),
        ("Inebriated Bubbles", 1223, "SPELLS\\BUBBLE_DRUNK.M2", 2000),
    ] {
        let effect = catalog
            .named(&format!("HARDCODED {name}"))
            .ok_or("effect")?;
        assert_eq!(effect.id(), id);
        let path = canonical_model_path(&effect.model_path()?.ok_or("model path")?)?;
        assert_eq!(path.as_str(), model_name);
        let model = DecodedM2Model::load_primary_profile(&mut store, &path)?;
        assert!(!model.animations().particles().is_empty());
        assert_eq!(model.animations().sequences()[0].duration_ms(), duration);
        for texture in model.textures() {
            if let Some(path) = texture.filename() {
                store.read(path)?;
            }
        }
    }
    Ok(())
}

fn fixture_table(rows: &[(u32, &str, &str)]) -> Vec<u8> {
    let mut strings = vec![0];
    let mut fields = Vec::new();
    for &(id, name, path) in rows {
        let name_offset = strings.len() as u32;
        strings.extend_from_slice(name.as_bytes());
        strings.push(0);
        let path_offset = strings.len() as u32;
        strings.extend_from_slice(path.as_bytes());
        strings.push(0);
        fields.extend([id, name_offset, path_offset]);
        fields.extend([9.0_f32, 2.0, 0.25, 4.0].map(f32::to_bits));
    }
    let mut bytes = b"WDBC".to_vec();
    for value in [rows.len() as u32, 7, 28, strings.len() as u32] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for value in fields {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend(strings);
    bytes
}
