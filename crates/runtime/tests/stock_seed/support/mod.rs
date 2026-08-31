//! Generated client archive layout for runtime integration tests.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use wow_mpq::{ArchiveBuilder, ListfileOption};

const REQUIRED_ARCHIVES: [&str; 10] = [
    "expansion.MPQ",
    "lichking.MPQ",
    "common.MPQ",
    "common-2.MPQ",
    "enUS/locale-enUS.MPQ",
    "enUS/speech-enUS.MPQ",
    "enUS/expansion-locale-enUS.MPQ",
    "enUS/lichking-locale-enUS.MPQ",
    "enUS/expansion-speech-enUS.MPQ",
    "enUS/lichking-speech-enUS.MPQ",
];

/// A complete temporary consolidated client data profile.
pub(crate) struct ClientFixture {
    root: PathBuf,
}

impl ClientFixture {
    /// Generates all archives required by runtime startup.
    pub(crate) fn new() -> Result<Self, Box<dyn Error>> {
        Self::with_common_files(&[])
    }

    /// Generates the profile with additional exact-path base asset payloads.
    pub(crate) fn with_common_files(
        common_files: &[(&str, &[u8])],
    ) -> Result<Self, Box<dyn Error>> {
        static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "solarity-runtime-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("Data/enUS"))?;
        for archive in REQUIRED_ARCHIVES {
            let additions = if archive == "common.MPQ" {
                common_files
            } else {
                &[]
            };
            build_archive(&root.join("Data").join(archive), archive, additions)?;
        }
        let addon_root = root.join("Interface/AddOns/Blizzard_RuntimeFixture");
        fs::create_dir_all(&addon_root)?;
        fs::write(
            addon_root.join("Blizzard_RuntimeFixture.pub"),
            b"signature marker",
        )?;
        Ok(Self { root })
    }

    /// Returns the generated data directory.
    pub(crate) fn data_root(&self) -> PathBuf {
        self.root.join("Data")
    }
}

impl Drop for ClientFixture {
    fn drop(&mut self) {
        let _cleanup_result = fs::remove_dir_all(&self.root);
    }
}

/// Creates one valid MPQ with a diagnostic marker.
fn build_archive(
    path: &Path,
    archive: &str,
    additions: &[(&str, &[u8])],
) -> Result<(), Box<dyn Error>> {
    let mut builder = ArchiveBuilder::new()
        .listfile_option(ListfileOption::Generate)
        .add_file_data(
            format!("fixture:{archive}").into_bytes(),
            "Solarity\\RuntimeFixture.txt",
        );
    for (asset_path, bytes) in additions {
        builder = builder.add_file_data(bytes.to_vec(), asset_path);
    }
    if archive == "enUS/locale-enUS.MPQ" {
        builder = builder.add_file_data(realm_category_dbc(), "DBFilesClient\\Cfg_Categories.dbc");
        builder =
            builder.add_file_data(realm_configuration_dbc(), "DBFilesClient\\Cfg_Configs.dbc");
        builder = builder.add_file_data(empty_wdbc(69), "DBFilesClient\\ChrRaces.dbc");
        builder = builder.add_file_data(empty_wdbc(60), "DBFilesClient\\ChrClasses.dbc");
        builder = builder.add_file_data(empty_wdbc(36), "DBFilesClient\\AreaTable.dbc");
        builder = builder.add_file_data(empty_wdbc(66), "DBFilesClient\\Map.dbc");
        builder = builder.add_file_data(empty_wdbc(15), "DBFilesClient\\Light.dbc");
        builder = builder.add_file_data(empty_wdbc(30), "DBFilesClient\\SoundEntries.dbc");
        builder = builder.add_file_data(empty_wdbc(24), "DBFilesClient\\SoundEntriesAdvanced.dbc");
        builder = builder.add_file_data(empty_wdbc(9), "DBFilesClient\\LightParams.dbc");
        builder = builder.add_file_data(empty_wdbc(3), "DBFilesClient\\LightSkybox.dbc");
        builder = builder.add_file_data(empty_wdbc(34), "DBFilesClient\\LightIntBand.dbc");
        builder = builder.add_file_data(empty_wdbc(34), "DBFilesClient\\LightFloatBand.dbc");
        builder = builder.add_file_data(empty_wdbc(16), "DBFilesClient\\CreatureDisplayInfo.dbc");
        builder = builder.add_file_data(
            empty_wdbc(21),
            "DBFilesClient\\CreatureDisplayInfoExtra.dbc",
        );
        builder = builder.add_file_data(empty_wdbc(28), "DBFilesClient\\CreatureModelData.dbc");
        builder = builder.add_file_data(empty_wdbc(10), "DBFilesClient\\ParticleColor.dbc");
        builder = builder.add_file_data(empty_wdbc(10), "DBFilesClient\\CharSections.dbc");
        builder = builder.add_file_data(empty_wdbc(6), "DBFilesClient\\CharHairGeosets.dbc");
        builder = builder.add_file_data(
            empty_wdbc(8),
            "DBFilesClient\\CharacterFacialHairStyles.dbc",
        );
        builder = builder.add_file_data(empty_wdbc(8), "DBFilesClient\\HelmetGeosetVisData.dbc");
        builder = builder.add_file_data(
            bootstrap_texture_blp(),
            "Interface\\Icons\\INV_Misc_QuestionMark.blp",
        );
        builder = builder.add_file_data(
            b"Bootstrap.xml\nAfter.lua\n".to_vec(),
            "Interface\\GlueXML\\GlueXML.toc",
        );
        builder = builder.add_file_data(
            b"## Interface: 30300\n## LoadOnDemand: 1\nRuntime.lua\n".to_vec(),
            "Interface\\AddOns\\Blizzard_RuntimeFixture\\Blizzard_RuntimeFixture.toc",
        );
        builder = builder.add_file_data(
            br#"<Ui><Frame name="GlueBootstrap" setAllPoints="true"><Layers>
  <Layer level="BACKGROUND"><Texture name="$parentTexture" file="Interface\Icons\INV_Misc_QuestionMark" setAllPoints="true"/></Layer>
</Layers><Frames>
  <Model name="$parentModel"/>
</Frames><Scripts><OnLoad>
  GlueBootstrapModel:SetModel("Solarity\\RuntimeFixture.txt")
  self.loaded = true
</OnLoad></Scripts></Frame></Ui>"#
                .to_vec(),
            "Interface\\GlueXML\\Bootstrap.xml",
        );
        builder = builder.add_file_data(
            br#"assert(GlueBootstrap.loaded)
assert(GlueBootstrapModel:GetModel() == "SOLARITY\\RUNTIMEFIXTURE.TXT")
GLUE_READY = true"#
                .to_vec(),
            "Interface\\GlueXML\\After.lua",
        );
    }
    builder.build(path)?;
    Ok(())
}

/// Builds one exact-layout enUS realm category for composition startup.
fn realm_category_dbc() -> Vec<u8> {
    let mut fields = [0_u32; 21];
    fields[0] = 1;
    fields[1] = 1;
    fields[2] = 1;
    fields[4] = 1;
    wdbc(&fields, b"\0United States\0")
}

/// Builds one exact-layout PvE realm configuration for composition startup.
fn realm_configuration_dbc() -> Vec<u8> {
    wdbc(&[1, 0, 0, 0], b"\0")
}

/// Builds an empty exact-layout table for metadata not exercised at startup.
fn empty_wdbc(field_count: u32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(21);
    bytes.extend_from_slice(b"WDBC");
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&field_count.to_le_bytes());
    bytes.extend_from_slice(&(field_count * 4).to_le_bytes());
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.push(0);
    bytes
}

/// Serializes one-row build-12340 WDBC fixture without runtime crate helpers.
fn wdbc(fields: &[u32], strings: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(20 + fields.len() * 4 + strings.len());
    bytes.extend_from_slice(b"WDBC");
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&(fields.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&((fields.len() as u32) * 4).to_le_bytes());
    bytes.extend_from_slice(&(strings.len() as u32).to_le_bytes());
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.extend_from_slice(strings);
    bytes
}

/// Builds a two-pixel BLP2/RAW3 texture for the asset-backed startup frame.
pub(crate) fn bootstrap_texture_blp() -> Vec<u8> {
    const HEADER_SIZE: u32 = 148;
    const PALETTE_SIZE: u32 = 256 * 4;
    const PIXEL_OFFSET: u32 = HEADER_SIZE + PALETTE_SIZE;
    const PIXEL_BYTES: u32 = 8;

    let mut bytes = Vec::with_capacity((PIXEL_OFFSET + PIXEL_BYTES) as usize);
    bytes.extend_from_slice(b"BLP2");
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&[3, 8, 8, 0]);
    bytes.extend_from_slice(&2_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&PIXEL_OFFSET.to_le_bytes());
    for _unused in 1..16 {
        bytes.extend_from_slice(&0_u32.to_le_bytes());
    }
    bytes.extend_from_slice(&PIXEL_BYTES.to_le_bytes());
    for _unused in 1..16 {
        bytes.extend_from_slice(&0_u32.to_le_bytes());
    }
    bytes.resize(PIXEL_OFFSET as usize, 0);
    bytes.extend_from_slice(&0xFFFF_0000_u32.to_le_bytes());
    bytes.extend_from_slice(&0xFF00_FF00_u32.to_le_bytes());
    bytes
}
