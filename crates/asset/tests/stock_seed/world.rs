//! Archive-backed minimap name translation compatibility.

use std::error::Error;

use solarity_asset::{
    ArchiveCatalog, AssetError, AssetPath, AssetStore, ClientDataRoot, Locale,
    MinimapTextureCatalog, TerrainTileIndex,
};

use crate::support::{Fixture, FixtureFile};

#[test]
fn minimap_translations_follow_archive_precedence_and_native_tile_names()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "Textures\\Minimap\\md5translate.trs",
            bytes: b"Kalimdor\\map40_28.blp\tbase.blp\nKalimdor\\map41_28.blp\tbase-only.blp\n",
        },
        FixtureFile {
            archive: "patch.MPQ",
            path: "Textures\\Minimap\\md5translate.trs",
            bytes: b"dir: Kalimdor\r\nKalimdor\\map40_28.blp\told.blp\r\n\
                     kalimdor\\MAP40_28.BLP\tupdated.blp\r\n\
                     Kalimdor\\map4_08.blp\tedge.blp\r\n\
                     ignored line\r\ndir: WMO\\Dungeon\r\n\
                     WMO\\Dungeon\\Room_003_00_01.blp\troom.blp\r\n\
                     \0Kalimdor\\map40_28.blp\tafter-nul.blp\n",
        },
    ])?;
    let mut store = mount(&fixture)?;
    let catalog = MinimapTextureCatalog::load(&mut store)?;
    let tile = TerrainTileIndex::new(40, 28).ok_or("tile")?;
    assert_eq!(
        catalog
            .terrain_texture("kALIMDor", tile)?
            .ok_or("terrain")?
            .as_str(),
        "TEXTURES\\MINIMAP\\UPDATED.BLP"
    );
    assert_eq!(
        catalog
            .terrain_texture("Kalimdor", TerrainTileIndex::new(4, 8).ok_or("edge tile")?)?
            .ok_or("edge")?
            .as_str(),
        "TEXTURES\\MINIMAP\\EDGE.BLP"
    );
    assert_eq!(
        catalog
            .texture(&AssetPath::new("wmo/dungeon/room_003_00_01.blp")?)
            .ok_or("room")?
            .as_str(),
        "TEXTURES\\MINIMAP\\ROOM.BLP"
    );
    assert!(
        catalog
            .terrain_texture(
                "Kalimdor",
                TerrainTileIndex::new(41, 28).ok_or("missing tile")?
            )?
            .is_none(),
        "the selected patch table replaces the older table rather than merging rows"
    );
    Ok(())
}

#[test]
fn missing_minimap_table_is_empty_but_invalid_archive_paths_are_rejected()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[])?;
    let catalog = MinimapTextureCatalog::load(&mut mount(&fixture)?)?;
    assert!(
        catalog
            .texture(&AssetPath::new("Kalimdor/map40_28.blp")?)
            .is_none()
    );
    let invalid = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "Textures\\Minimap\\md5translate.trs",
        bytes: b"Kalimdor\\map40_28.blp\t..\\outside.blp\n",
    }])?;
    assert!(matches!(
        MinimapTextureCatalog::load(&mut mount(&invalid)?),
        Err(AssetError::MinimapDecode { .. })
    ));
    Ok(())
}

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let root = ClientDataRoot::new(fixture.data_root())?;
    Ok(AssetStore::mount(ArchiveCatalog::discover(
        root,
        Locale::EnUs,
    )?)?)
}
