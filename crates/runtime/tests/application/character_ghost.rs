//! Refreshed server character flags propagate through metadata to Glue previews.

use super::RuntimeCharacterMetadata;
use crate::{test_network::WorldServer, test_support::ClientFixture};
use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};

#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with locally owned build-12340 archives"]
fn character_list_refresh_replaces_same_character_ghost_preview()
-> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("stock data root")?;
    let mut stock = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    let metadata = RuntimeCharacterMetadata::load(&mut stock)?;
    let fixture = ClientFixture::with_common_files(&[
        ("Interface/GlueXML/GlueXML.toc", b"Ghost.xml\n"),
        ("Interface/GlueXML/Ghost.xml", br#"<Ui><ModelFFX name="CharacterSelect"><Scripts><OnLoad>SetCharSelectModelFrame('CharacterSelect')</OnLoad></Scripts></ModelFFX></Ui>"#),
        ("Interface/Glues/Models/UI_Human/UI_Human.m2", b"selection model fixture"),
    ])?;
    let store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let glue = solarity_ui::GlueManager::start(store, (800, 600), false)?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let (server, mut network) = WorldServer::connect()
                .await
                .map_err(|error| error.to_string())?;
            for flags in [0_u32, 0x2000, 0x6000, 0x4000, 0x2000, 0] {
                let mut packet = vec![1];
                packet.extend(8_u64.to_le_bytes());
                packet.extend(b"WaterTest\0");
                packet.extend([1, 1, 0, 0, 0, 0, 0, 0, 80]);
                packet.extend([0; 24]); // location and guild
                packet.extend(flags.to_le_bytes());
                packet.extend([0; 4 + 1 + 12 + 23 * 9]);
                server
                    .exchange(vec![(0x3b, packet)], 0)
                    .await
                    .map_err(|error| error.to_string())?
                    .await?
                    .map_err(|error| error.to_string())?;
                let packet = network.receive_packet().await?;
                let directory = packet.character_directory()?.ok_or("character list")?;
                assert_eq!(directory.entries()[0].flags(), flags);
                glue.set_character_directory(metadata.project(&directory)?);
                let preview = glue
                    .character_selection_preview()
                    .ok_or("selection preview")?;
                assert_eq!(preview.guid(), 8);
                assert_eq!(preview.is_ghost(), flags & 0x2000 != 0);
            }
            Ok::<(), Box<dyn std::error::Error>>(())
        })
}
