//! Metadata seeks must not use weighted, available, or alias-payload durations.

use super::*;

#[test]
fn model_animation_duration_uses_authored_head_after_fallback() -> Result<(), Box<dyn Error>> {
    let mut dbc = b"WDBC".to_vec();
    for value in [1_u32, 8, 32, 1, 148, 0, 0, 0, 0x10, 5, 148, 0] {
        dbc.extend_from_slice(&value.to_le_bytes());
    }
    dbc.push(0);
    for flags in [0_u32, 0x40] {
        let mut bytes = animated_m2_bytes()?;
        let original = m2_array_offset(&bytes, 0x1c)?;
        let record = bytes[original..original + 64].to_vec();
        let offset = bytes.len() as u32;
        for index in 0..2 {
            let start = bytes.len();
            bytes.extend_from_slice(&record);
            bytes[start + 2..start + 4].copy_from_slice(&[4_u16, 0][index].to_le_bytes());
            bytes[start + 4..start + 8].copy_from_slice(&[1_234_u32, 2_000][index].to_le_bytes());
            bytes[start + 12..start + 16].copy_from_slice(&[flags, 0x20][index].to_le_bytes());
            bytes[start + 16..start + 20].copy_from_slice(&[0_u32, 32_767][index].to_le_bytes());
            bytes[start + 60..start + 62].copy_from_slice(&[1_u16, u16::MAX][index].to_le_bytes());
            bytes[start + 62..start + 64].copy_from_slice(&1_u16.to_le_bytes());
        }
        bytes[0x1c..0x20].copy_from_slice(&2_u32.to_le_bytes());
        bytes[0x20..0x24].copy_from_slice(&offset.to_le_bytes());
        let skin = skin_bytes(32, &[0, 1, 2])?;
        let fixture = Fixture::new(&[
            FixtureFile {
                archive: "common.MPQ",
                path: "Creature\\Solarity\\Metadata.m2",
                bytes: &bytes,
            },
            FixtureFile {
                archive: "common.MPQ",
                path: "Creature\\Solarity\\Metadata00.skin",
                bytes: &skin,
            },
            FixtureFile {
                archive: "common.MPQ",
                path: "DBFilesClient\\AnimationData.dbc",
                bytes: &dbc,
            },
        ])?;
        let archive =
            ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
        let mut store = AssetStore::mount(archive)?;
        let model = DecodedM2Model::load(
            &mut store,
            &AssetPath::new("Creature\\Solarity\\Metadata.m2")?,
        )?;
        let catalog = AnimationDataCatalog::load(&mut store)?;
        let animations = model.animations();
        assert_eq!(animations.select_model_sequence(5, 0), Some(1));
        assert_eq!(animations.sequence_for_variation(5, 0), Some(1));
        if flags == 0 {
            assert_eq!(animations.is_sequence_available(0), Some(false));
        } else {
            assert_eq!(animations.resolve_sequence_alias(0), Some(1));
        }
        assert_eq!(
            animations.model_animation_duration_ms(&catalog, 5),
            Some(1_234)
        );
        assert_eq!(
            animations.model_animation_duration_ms(&catalog, 148),
            Some(1_234)
        );
    }
    Ok(())
}
