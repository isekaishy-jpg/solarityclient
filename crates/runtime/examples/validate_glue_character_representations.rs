//! Validates every stock character-creation representation against installed data.

use std::cell::RefCell;
use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;
use std::rc::Rc;

use glam::Vec3;
use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetStore, AssetStoreHandle, CharacterAppearanceCatalog,
    CharacterRaceCatalog, CharacterStartOutfitCatalog, ClientDataRoot, CreatureCatalog,
    CreatureFamilyCatalog, HelmetGeosetVisibilityCatalog, ItemDefinitionCatalog,
    ItemDisplayCatalog, ItemVisualCatalog, Locale, ParticleColorCatalog,
};
use solarity_cpu::BlizzardRand;
use solarity_ecs::{ActiveWorld, ObjectKind, WorldBootstrap, WorldMapId, WorldTransform};
use solarity_runtime::{
    RuntimePlayerCatalogs, RuntimePlayerItemCatalogs, RuntimePlayerPoll, RuntimePlayerPresentation,
};
use solarity_systems::project_object_fields;
use solarity_ui::{
    UiCharacterCreationPreview, UiCharacterCreationState, UiCharacterDirectory,
    UiCharacterEquipment, UiCharacterExpansion, UiCharacterInfo, UiCharacterPetPreview,
};

const CUSTOMIZATION_AXIS_COUNT: usize = 5;
const MAX_CUSTOMIZATION_VALUES: usize = 256;

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
    if arguments.next().is_some() {
        return Err(usage_error().into());
    }

    let catalog = ArchiveCatalog::discover(ClientDataRoot::new(data_root)?, locale)?;
    let mut store = AssetStore::mount(catalog)?;
    let animations = AnimationDataCatalog::load(&mut store)?;
    let creatures = CreatureCatalog::load(&mut store)?;
    let creature_families = CreatureFamilyCatalog::load(&mut store)?;
    let characters = CharacterAppearanceCatalog::load(&mut store)?;
    let races = CharacterRaceCatalog::load(&mut store)?;
    let human_male_display_id = races
        .race(1)
        .ok_or_else(|| invalid_data("playable Human race is absent".to_owned()))?
        .male_display_id();
    let helmet_visibility = HelmetGeosetVisibilityCatalog::load(&mut store)?;
    let start_outfits = CharacterStartOutfitCatalog::load(&mut store)?;
    let item_definitions = ItemDefinitionCatalog::load(&mut store)?;
    let item_displays = ItemDisplayCatalog::load(&mut store)?;
    let item_visuals = ItemVisualCatalog::load(&mut store)?;
    let particle_colors = ParticleColorCatalog::load(&mut store)?;
    let creation = UiCharacterCreationState::load(
        &mut store,
        false,
        Rc::new(RefCell::new(BlizzardRand::new(0x1234_5678))),
    )?;
    let assets = AssetStoreHandle::new(store);
    let mut presentation = RuntimePlayerPresentation::new(
        assets,
        RuntimePlayerCatalogs::new(
            animations,
            creatures,
            creature_families,
            characters,
            races,
            helmet_visibility,
            start_outfits,
            RuntimePlayerItemCatalogs::new(item_definitions, item_displays, item_visuals),
            particle_colors,
        ),
    );

    creation.set_expansion(UiCharacterExpansion::WRATH_OF_THE_LICH_KING);
    creation.reset()?;
    if creation.selected_sex() != 2 {
        return Err(invalid_data(
            "ResetCharCustomize replaced the stock default male selection".to_owned(),
        )
        .into());
    }
    creation.set_selected_sex(3)?;
    creation.reset()?;
    if creation.selected_sex() != 3 {
        return Err(invalid_data(
            "ResetCharCustomize replaced the current female selection".to_owned(),
        )
        .into());
    }
    creation.set_selected_sex(2)?;
    creation.reset()?;
    let available_races = creation.available_races();
    let available_classes = creation.available_classes();
    validate_bald_facial_hair_representation(
        &mut presentation,
        &creation,
        &available_races,
        &available_classes,
    )?;
    let mut outfit_count = 0_usize;
    let mut appearance_count = 0_usize;

    for (race_offset, race) in available_races.iter().enumerate() {
        if !race.2 {
            continue;
        }
        let race_index = one_based(race_offset)?;
        creation.set_selected_race(race_index)?;
        for sex in [2, 3] {
            creation.set_selected_sex(sex)?;
            for (class_offset, class) in available_classes.iter().enumerate() {
                if !class.2 {
                    continue;
                }
                let class_index = one_based(class_offset)?;
                if !creation.is_race_class_valid(race_index, class_index) {
                    continue;
                }
                creation.set_selected_class(class_index)?;
                validate_preview(&mut presentation, &creation.preview())?;
                outfit_count += 1;
            }
            for axis in 0..CUSTOMIZATION_AXIS_COUNT {
                appearance_count +=
                    validate_customization_axis(&mut presentation, &creation, axis)?;
            }
        }
    }
    validate_selection_representation(&mut presentation)?;
    validate_selection_to_world_transfer(&mut presentation, human_male_display_id)?;
    presentation.synchronize_character_creation(None)?;

    println!(
        "validated {outfit_count} race/class/gender outfits, {appearance_count} authored customization representations, and one complete enum-time equipment/pet representation"
    );
    Ok(())
}

/// Proves world entry consumes the already-composed selected character instead
/// of decoding its body, atlas, and attachment generation a second time.
fn validate_selection_to_world_transfer(
    presentation: &mut RuntimePlayerPresentation,
    body_display_id: u32,
) -> Result<(), Box<dyn Error>> {
    let guid = 0x42;
    let directory = UiCharacterDirectory::new(
        vec![UiCharacterInfo::new(
            guid,
            "TransferHuman".to_owned(),
            "Human".to_owned(),
            1,
            "Human".to_owned(),
            "Warrior".to_owned(),
            1,
            1,
            None,
            2,
            0,
            [0; 5],
            [UiCharacterEquipment::default(); 23],
            UiCharacterPetPreview::default(),
            0,
            0,
        )],
        "Human".to_owned(),
    );
    let preview = directory
        .selection_preview()
        .ok_or_else(|| invalid_data("world-transfer fixture produced no preview".to_owned()))?;
    if !presentation.synchronize_character_selection(Some(&preview))? {
        return Err(invalid_data(
            "world-transfer fixture did not publish its selected character".to_owned(),
        )
        .into());
    }
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        guid,
        "TransferHuman",
        Vec3::ZERO,
        0.0,
    ));
    let fields = [
        (4, 1.0_f32.to_bits()),
        (23, u32::from_le_bytes([1, 1, 0, 0])),
        (67, body_display_id),
        (68, body_display_id),
        (69, 0),
        (74, 0),
        (122, 0),
        (153, 0),
        (154, 0),
    ];
    world.create_object(
        guid,
        ObjectKind::Player,
        Some(WorldTransform::new(Vec3::ZERO, 0.0)),
        fields,
    )?;
    project_object_fields(&mut world, guid, fields)?;
    if presentation.synchronize(Some(&world))? != RuntimePlayerPoll::ModelLoaded {
        return Err(invalid_data(
            "selected character did not transfer into active-world residency".to_owned(),
        )
        .into());
    }
    if !presentation.synchronize_character_selection(Some(&preview))? {
        return Err(invalid_data(
            "world entry retained a duplicate Glue character generation".to_owned(),
        )
        .into());
    }
    Ok(())
}

/// Locks the stock-valid Gnome case that previously reached Vulkan with a
/// selected facial-hair draw and an unresolved bald-hair texture slot.
fn validate_bald_facial_hair_representation(
    presentation: &mut RuntimePlayerPresentation,
    creation: &UiCharacterCreationState,
    races: &[(String, String, bool)],
    classes: &[(String, String, bool)],
) -> Result<(), Box<dyn Error>> {
    let race_offset = races
        .iter()
        .position(|race| race.1.eq_ignore_ascii_case("Gnome"))
        .ok_or_else(|| invalid_data("playable Gnome race is absent".to_owned()))?;
    let race_index = one_based(race_offset)?;
    creation.set_selected_race(race_index)?;
    creation.set_selected_sex(2)?;
    let class_offset = classes
        .iter()
        .enumerate()
        .find(|(offset, class)| {
            class.1.eq_ignore_ascii_case("DEATHKNIGHT")
                && class.2
                && one_based(*offset)
                    .is_ok_and(|index| creation.is_race_class_valid(race_index, index))
        })
        .map(|(offset, _class)| offset)
        .ok_or_else(|| invalid_data("playable Gnome Death Knight is absent".to_owned()))?;
    creation.set_selected_class(one_based(class_offset)?)?;
    for (axis, target) in [7_u8, 0, 0, 8, 5].into_iter().enumerate() {
        for _attempt in 0..MAX_CUSTOMIZATION_VALUES {
            if creation.preview().appearance()[axis] == target {
                break;
            }
            creation.cycle_customization(one_based(axis)?, 1)?;
        }
        if creation.preview().appearance()[axis] != target {
            return Err(invalid_data(format!(
                "Gnome customization axis {} cannot select value {target}",
                axis + 1
            ))
            .into());
        }
    }
    let preview = creation.preview();
    validate_preview(presentation, &preview)?;
    println!(
        "validated bald facial-hair representation race={} gender={} appearance={:?}",
        preview.race_id(),
        preview.gender_id(),
        preview.appearance(),
    );
    Ok(())
}

/// Resolves one real playerbots-style enum row spanning all modeled slot kinds.
fn validate_selection_representation(
    presentation: &mut RuntimePlayerPresentation,
) -> Result<(), Box<dyn Error>> {
    let equipment = [
        UiCharacterEquipment::new(65_131, 1, 0),
        UiCharacterEquipment::new(64_190, 2, 0),
        UiCharacterEquipment::new(64_829, 3, 0),
        UiCharacterEquipment::new(7_904, 4, 0),
        UiCharacterEquipment::new(64_840, 5, 0),
        UiCharacterEquipment::new(65_035, 6, 0),
        UiCharacterEquipment::new(64_832, 7, 0),
        UiCharacterEquipment::new(64_822, 8, 0),
        UiCharacterEquipment::new(64_421, 9, 0),
        UiCharacterEquipment::new(64_827, 10, 0),
        UiCharacterEquipment::new(64_225, 11, 0),
        UiCharacterEquipment::new(64_230, 11, 0),
        UiCharacterEquipment::new(68_106, 12, 0),
        UiCharacterEquipment::new(68_109, 12, 0),
        UiCharacterEquipment::new(28_951, 16, 0),
        UiCharacterEquipment::new(64_554, 17, 0),
        UiCharacterEquipment::default(),
        UiCharacterEquipment::new(64_356, 15, 0),
        UiCharacterEquipment::new(20_621, 19, 0),
        UiCharacterEquipment::new(56_653, 18, 0),
        UiCharacterEquipment::default(),
        UiCharacterEquipment::default(),
        UiCharacterEquipment::default(),
    ];
    let directory = UiCharacterDirectory::new(
        vec![UiCharacterInfo::new(
            1,
            "ValidationHunter".to_owned(),
            "Night Elf".to_owned(),
            4,
            "NightElf".to_owned(),
            "Hunter".to_owned(),
            3,
            80,
            None,
            2,
            0,
            [0, 1, 6, 5, 2],
            equipment,
            UiCharacterPetPreview::new(2_711, 80, 25),
            0,
            0,
        )],
        "Human".to_owned(),
    );
    let preview = directory
        .selection_preview()
        .ok_or_else(|| invalid_data("selection fixture produced no preview".to_owned()))?;
    if !presentation.synchronize_character_selection(Some(&preview))? {
        return Err(invalid_data(
            "complete selection fixture did not replace the creation representation".to_owned(),
        )
        .into());
    }
    presentation.validate_glue_character_geometry_textures()?;
    if presentation.synchronize_character_selection(Some(&preview))? {
        return Err(invalid_data(
            "unchanged selection fixture rebuilt its resident representation".to_owned(),
        )
        .into());
    }
    Ok(())
}

/// Walks one byte-valued appearance axis until stock wraparound returns to its start.
fn validate_customization_axis(
    presentation: &mut RuntimePlayerPresentation,
    creation: &UiCharacterCreationState,
    axis: usize,
) -> Result<usize, Box<dyn Error>> {
    let initial = creation.preview().appearance()[axis];
    let mut count = 0_usize;
    loop {
        creation.cycle_customization(one_based(axis)?, 1)?;
        let preview = creation.preview();
        validate_preview(presentation, &preview)?;
        count += 1;
        if preview.appearance()[axis] == initial {
            return Ok(count);
        }
        if count == MAX_CUSTOMIZATION_VALUES {
            return Err(invalid_data(format!(
                "customization axis {} did not wrap within its byte domain",
                axis + 1
            ))
            .into());
        }
    }
}

/// Requires the production character-composition boundary to admit one preview.
fn validate_preview(
    presentation: &mut RuntimePlayerPresentation,
    preview: &UiCharacterCreationPreview,
) -> Result<(), Box<dyn Error>> {
    presentation.synchronize_character_creation(Some(preview))?;
    presentation.validate_glue_character_geometry_textures()?;
    Ok(())
}

/// Converts a zero-based collection offset to Glue's one-based selector domain.
fn one_based(offset: usize) -> Result<u32, IoError> {
    u32::try_from(offset + 1).map_err(|_source| {
        invalid_data("selector index exceeds the Glue integer domain".to_owned())
    })
}

fn invalid_data(message: String) -> IoError {
    IoError::new(ErrorKind::InvalidData, message)
}

fn usage_error() -> IoError {
    IoError::new(
        ErrorKind::InvalidInput,
        "usage: validate_glue_character_representations <Data directory> <locale>",
    )
}
