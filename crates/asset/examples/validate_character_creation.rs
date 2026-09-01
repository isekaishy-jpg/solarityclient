//! Validates stock character-creation metadata against an installed client.

use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;

use solarity_asset::{
    ArchiveCatalog, AssetStore, CharacterAppearanceCatalog, CharacterBaseCatalog,
    CharacterClassCatalog, CharacterFactionCatalog, CharacterRaceCatalog,
    CharacterStartOutfitCatalog, ClientDataRoot, Locale,
};

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
    let races = CharacterRaceCatalog::load(&mut store)?;
    let classes = CharacterClassCatalog::load(&mut store)?;
    let combinations = CharacterBaseCatalog::load(&mut store)?;
    let factions = CharacterFactionCatalog::load(&mut store)?;
    let outfits = CharacterStartOutfitCatalog::load(&mut store)?;
    let appearances = CharacterAppearanceCatalog::load(&mut store)?;

    for race in races.physical_races() {
        let faction = factions.group_for_template(race.faction_id());
        let authored_classes = combinations
            .entries()
            .iter()
            .filter(|entry| u32::from(entry.race_id()) == race.id())
            .map(|entry| entry.class_id())
            .collect::<Vec<_>>();
        println!(
            "race={} file={} flags={:#010X} expansion={} faction={} classes={authored_classes:?}",
            race.id(),
            race.client_file_string(),
            race.flags(),
            race.required_expansion(),
            faction.map_or("<none>", |group| group.internal_name()),
        );
    }
    for class in classes.physical_classes() {
        println!(
            "class={} file={} expansion={}",
            class.id(),
            class.file_string(),
            class.required_expansion()
        );
    }

    for entry in combinations.entries() {
        if races.race(u32::from(entry.race_id())).is_none() {
            return Err(format!("CharBaseInfo references absent race {}", entry.race_id()).into());
        }
        if classes.class(u32::from(entry.class_id())).is_none() {
            return Err(
                format!("CharBaseInfo references absent class {}", entry.class_id()).into(),
            );
        }
    }
    for entry in combinations.entries() {
        for gender_id in 0..=1 {
            if outfits
                .outfit(entry.race_id(), entry.class_id(), gender_id)
                .is_none()
            {
                return Err(format!(
                    "CharStartOutfit omits race/class/gender {}/{}/{gender_id}",
                    entry.race_id(),
                    entry.class_id()
                )
                .into());
            }
        }
    }
    let mut appearance_sets = 0_usize;
    for race in races.physical_races() {
        if !combinations
            .entries()
            .iter()
            .any(|entry| u32::from(entry.race_id()) == race.id())
        {
            continue;
        }
        for gender_id in 0..=1 {
            validate_appearance(&appearances, race.id(), gender_id)?;
            appearance_sets += 1;
        }
    }
    println!(
        "validated {} races, {} classes, {} race/class rows, {} starter outfits, and {appearance_sets} playable race/gender appearance sets",
        races.races().len(),
        classes.classes().len(),
        combinations.entries().len(),
        outfits.outfits().len()
    );
    Ok(())
}

fn validate_appearance(
    catalog: &CharacterAppearanceCatalog,
    race_id: u32,
    gender_id: u32,
) -> Result<(), Box<dyn Error>> {
    let skins = catalog.player_skin_colors(race_id, gender_id);
    if skins.is_empty() {
        return Err(format!("race/gender {race_id}/{gender_id} has no player skin colors").into());
    }
    for skin in skins {
        if catalog.player_faces(race_id, gender_id, skin).is_empty() {
            return Err(format!(
                "race/gender {race_id}/{gender_id} skin {skin} has no player faces"
            )
            .into());
        }
    }

    let hair_styles = catalog.player_hair_styles(race_id, gender_id);
    if hair_styles.is_empty() {
        return Err(format!("race/gender {race_id}/{gender_id} has no player hair styles").into());
    }
    for style in hair_styles {
        if catalog
            .player_hair_colors(race_id, gender_id, style)
            .is_empty()
        {
            return Err(format!(
                "race/gender {race_id}/{gender_id} hair style {style} has no player colors"
            )
            .into());
        }
    }

    let facial_hair = catalog.player_facial_hair_styles(race_id, gender_id);
    if facial_hair.is_empty() {
        return Err(
            format!("race/gender {race_id}/{gender_id} has no facial-feature styles").into(),
        );
    }
    for style in facial_hair {
        if catalog
            .facial_hair_style(race_id, gender_id, style)
            .is_none()
        {
            return Err(format!(
                "race/gender {race_id}/{gender_id} facial style {style} has no exact geometry row"
            )
            .into());
        }
    }
    Ok(())
}

fn usage_error() -> IoError {
    IoError::new(
        ErrorKind::InvalidInput,
        "usage: validate_character_creation <Data directory> <locale>",
    )
}
