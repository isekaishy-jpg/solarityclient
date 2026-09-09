//! Native unit body scale, separate from OBJECT_FIELD_SCALE_X and mount scale.

use solarity_asset::{CharacterRaceCatalog, CreatureCatalog, CreatureFamilyDefinition};
use solarity_ecs::{ActiveWorld, ObjectFields};

/// Resolves the body multiplier stored by build 12340 at `Unit_C + 0xB3C`.
///
/// `722AE0`, `71C110`, and `71C050` join the active display, model, optional
/// NPC race display, and bound creature family. Missing DBC scale providers
/// use one; a missing family leaves the authored body scale intact. This does
/// not include the independently animated object scale or mounted rider scale.
/// Returns `None` only when the unit's authoritative field table is absent.
#[must_use]
pub fn resolve_unit_body_scale(
    world: &ActiveWorld,
    guid: u64,
    creatures: &CreatureCatalog,
    races: &CharacterRaceCatalog,
    family: Option<CreatureFamilyDefinition>,
) -> Option<f32> {
    let entity = world.entity_by_guid(guid)?;
    let fields = world.storage().get::<&ObjectFields>(entity).ok()?;
    // Native +0xD0 points past the six common object words: +0xC0 is
    // absolute UNIT_FIELD_LEVEL 54; +0x114 is UNIT_FIELD_PETNUMBER 75.
    Some(body_scale(
        fields.get(67),
        fields.get(54) as i32,
        fields.get(75),
        creatures,
        races,
        family,
    ))
}

/// Retains extended intermediates until native's final body-scale float store.
fn body_scale(
    display_id: u32,
    level: i32,
    pet_number: u32,
    creatures: &CreatureCatalog,
    races: &CharacterRaceCatalog,
    family: Option<CreatureFamilyDefinition>,
) -> f32 {
    let Some(display) = creatures.display(display_id) else {
        return 1.0;
    };
    let Some(model) = creatures.model(display.model_id()) else {
        return 1.0;
    };
    let race_scale = creatures
        .display_extra(display.extended_display_info_id())
        .and_then(|extra| {
            let race = races.race(extra.race_id())?;
            let display_id = match extra.gender_id() {
                0 => race.male_display_id(),
                1 => race.female_display_id(),
                _ => 0,
            };
            creatures.display(display_id)
        })
        .map_or(1.0, |display| display.model_scale());
    let authored =
        f64::from(race_scale) * f64::from(display.model_scale()) * f64::from(model.model_scale());
    let authored = if authored <= 0.0 { 1.0 } else { authored };
    let Some(family) = family else {
        return authored as f32;
    };
    let (minimum, minimum_level) = family.minimum_scale();
    let (maximum, maximum_level) = family.maximum_scale();
    let minimum_level = minimum_level as i32;
    let width = (maximum_level as i32).wrapping_sub(minimum_level);
    // Preserve the original ordered signed clamps even for equal/reversed
    // intervals. The character-selection pet preview has different rules.
    let offset = if level < minimum_level {
        0
    } else {
        level.wrapping_sub(minimum_level)
    }
    .min(width);
    let progress = if width == 0 {
        0.0
    } else {
        f64::from(offset) / f64::from(width)
    };
    let family_scale = (f64::from(maximum) - f64::from(minimum)) * progress + f64::from(minimum);
    if authored < family_scale || pet_number != 0 {
        family_scale as f32
    } else {
        authored as f32
    }
}
