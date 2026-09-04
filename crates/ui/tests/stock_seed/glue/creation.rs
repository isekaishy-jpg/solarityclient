//! Build-12340 creation random consumption and retained selection contracts.

#[path = "creation/fixture.rs"]
mod fixture;

use std::error::Error;

use solarity_cpu::BlizzardRand;
use solarity_ui::UiCharacterExpansion;

use fixture::CreationFixture;

/// 0x004E17F0 rolls skin, face, hair color, hair style, then facial features.
#[test]
fn randomize_button_uses_scaled_words_in_stock_axis_order() -> Result<(), Box<dyn Error>> {
    let fixture = CreationFixture::new()?;
    fixture.state.randomize_customization()?;
    assert_eq!(fixture.state.preview().appearance(), [0, 3, 0, 0, 1]);
    assert_eq!(fixture.random.borrow_mut().next_u32(), 0x4518_56db);
    Ok(())
}

/// 0x004DFF10 rejects locked race draws; 0x004E1FD0 rolls class after appearance.
#[test]
fn reset_rolls_sex_rejects_locked_races_and_uses_base_class_order() -> Result<(), Box<dyn Error>> {
    let fixture = CreationFixture::new()?;
    fixture.state.set_selected_sex(3)?;
    *fixture.random.borrow_mut() = BlizzardRand::new(0x1234_5678);
    fixture.state.reset()?;
    assert_eq!(fixture.state.selected_sex(), 2);
    assert_eq!(fixture.state.selected_race(), 1);
    assert_eq!(fixture.state.selected_class(), 2);
    assert_eq!(fixture.state.preview().appearance(), [0, 1, 1, 2, 1]);
    assert_eq!(fixture.random.borrow_mut().next_u32(), 0xd22f_9d6a);
    Ok(())
}

/// 0x004E1540 indexes its saved appearance by race/sex and applies the live class.
#[test]
fn sex_preferences_survive_an_intervening_class_change() -> Result<(), Box<dyn Error>> {
    let fixture = CreationFixture::new()?;
    fixture.state.randomize_customization()?;
    let appearance = fixture.state.preview().appearance();
    fixture.state.set_selected_sex(3)?;
    fixture.state.set_selected_class(2)?;
    let random = *fixture.random.borrow();
    fixture.state.set_selected_sex(2)?;
    assert_eq!(fixture.state.preview().appearance(), appearance);
    assert_eq!(fixture.state.selected_class(), 2);
    assert_eq!(*fixture.random.borrow(), random);
    Ok(())
}

/// 0x004E1740 retains customization and 0x004E9D50 validates without new rolls.
#[test]
fn class_changes_preserve_appearance_and_random_state() -> Result<(), Box<dyn Error>> {
    let fixture = CreationFixture::new()?;
    fixture.state.randomize_customization()?;
    let appearance = fixture.state.preview().appearance();
    let random = *fixture.random.borrow();
    fixture.state.set_selected_class(2)?;
    assert_eq!(fixture.state.preview().appearance(), appearance);
    assert_eq!(*fixture.random.borrow(), random);
    fixture.state.set_selected_class(1)?;
    assert_eq!(fixture.state.preview().appearance(), appearance);
    assert_eq!(*fixture.random.borrow(), random);
    Ok(())
}

/// 0x004E9D50 maps invalid values by their old ordinal instead of rerolling.
#[test]
fn leaving_death_knight_repairs_exclusive_colors_without_randomness() -> Result<(), Box<dyn Error>>
{
    let fixture = CreationFixture::new()?;
    fixture
        .state
        .set_expansion(UiCharacterExpansion::WRATH_OF_THE_LICH_KING);
    fixture.state.set_selected_class(3)?;
    for _ in 0..3 {
        fixture.state.cycle_customization(1, 1)?;
    }
    for _ in 0..6 {
        fixture.state.cycle_customization(4, 1)?;
    }
    assert_eq!(fixture.state.preview().appearance()[0], 3);
    assert_eq!(fixture.state.preview().appearance()[3], 6);
    let random = *fixture.random.borrow();
    fixture.state.set_selected_class(1)?;
    assert_eq!(fixture.state.preview().appearance()[0], 0);
    assert_eq!(fixture.state.preview().appearance()[3], 0);
    assert_eq!(*fixture.random.borrow(), random);
    Ok(())
}

/// 0x004E01F0 uses only delta's sign, including a no-op for zero.
#[test]
fn customization_cycle_uses_direction_instead_of_delta_magnitude() -> Result<(), Box<dyn Error>> {
    let fixture = CreationFixture::new()?;
    fixture.state.cycle_customization(2, 3)?;
    assert_eq!(fixture.state.preview().appearance()[1], 1);
    fixture.state.cycle_customization(2, -9)?;
    assert_eq!(fixture.state.preview().appearance()[1], 0);
    fixture.state.cycle_customization(2, 0)?;
    assert_eq!(fixture.state.preview().appearance()[1], 0);
    Ok(())
}

/// Skin cycling at 0x004EB150 skips candidates lacking the current face/underwear.
#[test]
fn skin_cycle_preserves_face_and_skips_missing_inputs() -> Result<(), Box<dyn Error>> {
    let fixture = CreationFixture::new()?;
    fixture.state.set_selected_race(2)?;
    assert_eq!(fixture.state.preview().appearance()[..2], [0, 0]);
    fixture.state.cycle_customization(1, 1)?;
    assert_eq!(fixture.state.preview().appearance()[..2], [0, 0]);
    fixture.state.cycle_customization(2, 1)?;
    fixture.state.cycle_customization(1, 1)?;
    assert_eq!(fixture.state.preview().appearance()[..2], [1, 1]);
    fixture.state.cycle_customization(1, 1)?;
    assert_eq!(fixture.state.preview().appearance()[..2], [0, 1]);
    Ok(())
}

/// 0x004F0490 selects a style's first color and feature when color cannot persist.
#[test]
fn hair_style_cycle_repairs_its_color_and_feature_without_a_roll() -> Result<(), Box<dyn Error>> {
    let fixture = CreationFixture::new()?;
    fixture.state.set_selected_race(2)?;
    assert_eq!(fixture.state.preview().appearance(), [0, 0, 0, 4, 1]);
    fixture.state.cycle_customization(4, 1)?;
    fixture.state.cycle_customization(4, 1)?;
    fixture.state.cycle_customization(5, 1)?;
    let random = *fixture.random.borrow();
    fixture.state.cycle_customization(3, 1)?;
    assert_eq!(fixture.state.preview().appearance(), [0, 0, 1, 2, 0]);
    assert_eq!(*fixture.random.borrow(), random);
    Ok(())
}

/// 0x004EBCA0 branches on the current feature's allocated texture range.
#[test]
fn facial_cycle_crosses_geometry_and_texture_ranges_without_randomness()
-> Result<(), Box<dyn Error>> {
    let fixture = CreationFixture::new()?;
    fixture
        .state
        .set_expansion(UiCharacterExpansion::WRATH_OF_THE_LICH_KING);
    fixture.state.set_selected_race(3)?;
    assert_eq!(fixture.state.preview().appearance(), [0, 0, 0, 4, 0]);
    let random = *fixture.random.borrow();
    fixture.state.cycle_customization(5, 1)?;
    assert_eq!(fixture.state.preview().appearance()[3..], [4, 1]);
    fixture.state.cycle_customization(4, 1)?;
    fixture.state.cycle_customization(4, 1)?;
    fixture.state.cycle_customization(5, 1)?;
    assert_eq!(fixture.state.preview().appearance()[3..], [0, 2]);
    fixture.state.cycle_customization(5, 1)?;
    assert_eq!(fixture.state.preview().appearance()[3..], [2, 1]);
    assert_eq!(*fixture.random.borrow(), random);
    Ok(())
}

/// Stock face cycling can enter a DK-only skin and retains the explicit skin origin.
#[test]
fn face_cycle_crosses_skin_ranges_and_restores_the_explicit_skin() -> Result<(), Box<dyn Error>> {
    let fixture = CreationFixture::new()?;
    fixture
        .state
        .set_expansion(UiCharacterExpansion::WRATH_OF_THE_LICH_KING);
    fixture.state.set_selected_race(3)?;
    fixture.state.set_selected_class(3)?;
    assert_eq!(fixture.state.preview().appearance()[..2], [0, 2]);
    fixture.state.cycle_customization(1, 1)?;
    let random = *fixture.random.borrow();
    fixture.state.cycle_customization(2, 1)?;
    assert_eq!(fixture.state.preview().appearance()[..2], [1, 3]);
    fixture.state.cycle_customization(2, 1)?;
    assert_eq!(fixture.state.preview().appearance()[..2], [3, 0]);
    fixture.state.cycle_customization(2, -1)?;
    assert_eq!(fixture.state.preview().appearance()[..2], [1, 3]);
    assert_eq!(*fixture.random.borrow(), random);
    Ok(())
}
