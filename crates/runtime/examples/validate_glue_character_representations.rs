//! Validates every stock character-creation representation against installed data.

use std::cell::RefCell;
use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;
use std::rc::Rc;

use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetStore, AssetStoreHandle, CharacterAppearanceCatalog,
    CharacterRaceCatalog, CharacterStartOutfitCatalog, ClientDataRoot, CreatureCatalog,
    CreatureFamilyCatalog, HelmetGeosetVisibilityCatalog, ItemDefinitionCatalog,
    ItemDisplayCatalog, ItemVisualCatalog, Locale, ParticleColorCatalog,
};
use solarity_cpu::BlizzardRand;
use solarity_runtime::{
    RuntimePlayerCatalogs, RuntimePlayerItemCatalogs, RuntimePlayerPresentation,
};
use solarity_ui::{UiCharacterCreationPreview, UiCharacterCreationState, UiCharacterExpansion};

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
    let available_races = creation.available_races();
    let available_classes = creation.available_classes();
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
    presentation.synchronize_character_creation(None)?;

    println!(
        "validated {outfit_count} race/class/gender outfits and {appearance_count} authored customization representations"
    );
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
