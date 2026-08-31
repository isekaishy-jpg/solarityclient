//! External stock-compatibility tests for key assignment command streams.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    UiBindingAction, UiBindingAssignmentError, UiBindingAssignments, UiBindingCatalog,
    UiBindingMode,
};

use crate::support::{Fixture, FixtureFile};

const BINDINGS_XML: &[u8] = br#"<Bindings>
  <Binding name="MOVEFORWARD">MoveForwardStart()</Binding>
  <Binding name="STRAFELEFT">StrafeLeftStart()</Binding>
  <ModifiedClick action="SELFCAST" default="ALT"/>
  <ModifiedClick action="FOCUSCAST" default="NONE"/>
</Bindings>"#;

/// Archive defaults retain exact key spelling and seed XML-owned click actions.
#[test]
fn default_assignments_load_stock_archive_commands() -> Result<(), Box<dyn Error>> {
    let fixture = binding_fixture(
        b"\xEF\xBB\xBFbind W MOVEFORWARD\r\nbind UP MOVEFORWARD\r\nbind CTRL-- STRAFELEFT\r\n",
    )?;
    let mut assets = mount(&fixture)?;
    let catalog = UiBindingCatalog::load_builtin(&mut assets)?;

    let assignments = UiBindingAssignments::load_defaults(&mut assets, &catalog)?;

    assert_eq!(assignments.mode(), UiBindingMode::Default);
    assert_eq!(assignments.bindings().len(), 3);
    assert_eq!(assignments.bindings()[2].key().as_str(), "CTRL--");
    assert!(matches!(
        assignments.binding_for("W").map(|binding| binding.action()),
        Some(UiBindingAction::Command(command)) if command == "MOVEFORWARD"
    ));
    assert_eq!(
        assignments
            .modified_click("SELFCAST")
            .map(|assignment| assignment.chord().as_str()),
        Some("ALT")
    );
    assert!(
        assignments
            .modified_click("FOCUSCAST")
            .is_some_and(|assignment| assignment.chord().is_none())
    );
    Ok(())
}

/// Saved commands apply in source order and preserve every dynamic action kind.
#[test]
fn character_assignments_apply_stock_dynamic_actions_and_last_write() -> Result<(), Box<dyn Error>>
{
    let fixture = binding_fixture(b"bind W MOVEFORWARD\n")?;
    let mut assets = mount(&fixture)?;
    let catalog = UiBindingCatalog::load_builtin(&mut assets)?;
    let source = r#"BINDINGMODE 2
bind W MOVEFORWARD
bind W STRAFELEFT
bind CTRL-1 SPELL Frostbolt
bind CTRL-2 ITEM Hearthstone
bind CTRL-3 MACRO Dungeon Party
bind CTRL-4 CLICK TestButton:RightButton
modifiedclick SELFCAST CTRL-BUTTON1
"#;

    let assignments = UiBindingAssignments::parse(source, UiBindingMode::Character, &catalog)?;

    assert_eq!(assignments.mode(), UiBindingMode::Character);
    assert_eq!(assignments.bindings().len(), 5);
    assert!(matches!(
        assignments.binding_for("W").map(|binding| binding.action()),
        Some(UiBindingAction::Command(command)) if command == "STRAFELEFT"
    ));
    assert!(matches!(
        assignments.binding_for("CTRL-1").map(|binding| binding.action()),
        Some(UiBindingAction::Spell(name)) if name == "Frostbolt"
    ));
    assert!(matches!(
        assignments.binding_for("CTRL-2").map(|binding| binding.action()),
        Some(UiBindingAction::Item(name)) if name == "Hearthstone"
    ));
    assert!(matches!(
        assignments.binding_for("CTRL-3").map(|binding| binding.action()),
        Some(UiBindingAction::Macro(name)) if name == "Dungeon Party"
    ));
    assert!(matches!(
        assignments.binding_for("CTRL-4").map(|binding| binding.action()),
        Some(UiBindingAction::Click { button, mouse_button })
            if button == "TestButton" && mouse_button == "RightButton"
    ));
    assert_eq!(
        assignments
            .modified_click("SELFCAST")
            .map(|assignment| assignment.chord().as_str()),
        Some("CTRL-BUTTON1")
    );
    Ok(())
}

/// Unknown commands, mismatched modes, invalid tokens, and undeclared clicks fail.
#[test]
fn assignment_parser_rejects_non_stock_records() -> Result<(), Box<dyn Error>> {
    let fixture = binding_fixture(b"bind W MOVEFORWARD\n")?;
    let mut assets = mount(&fixture)?;
    let catalog = UiBindingCatalog::load_builtin(&mut assets)?;
    let invalid_sources = [
        "BINDINGMODE 1\n",
        "BINDINGMODE 2\nBINDINGMODE 2\n",
        "unbind W\n",
        "bind none MOVEFORWARD\n",
        "bind W moveforward\n",
        "modifiedclick UNKNOWN ALT\n",
        "modifiedclick SELFCAST META\n",
    ];

    for source in invalid_sources {
        assert!(matches!(
            UiBindingAssignments::parse(source, UiBindingMode::Character, &catalog),
            Err(UiBindingAssignmentError::Record { .. })
        ));
    }
    Ok(())
}

/// Binding mode zero is explicit default state rather than an account fallback.
#[test]
fn explicit_default_binding_mode_is_preserved() -> Result<(), Box<dyn Error>> {
    let fixture = binding_fixture(b"bind W MOVEFORWARD\n")?;
    let mut assets = mount(&fixture)?;
    let catalog = UiBindingCatalog::load_builtin(&mut assets)?;

    let assignments = UiBindingAssignments::parse(
        "BINDINGMODE 0\nbind W MOVEFORWARD\n",
        UiBindingMode::Default,
        &catalog,
    )?;

    assert_eq!(assignments.mode().stock_value(), 0);
    assert!(assignments.binding_for("W").is_some());
    Ok(())
}

fn binding_fixture(default_bindings: &[u8]) -> Result<Fixture, Box<dyn Error>> {
    Fixture::new(&[
        FixtureFile {
            path: "Interface\\FrameXML\\Bindings.xml",
            bytes: BINDINGS_XML,
        },
        FixtureFile {
            path: "WTF\\DefaultBindings.wtf",
            bytes: default_bindings,
        },
    ])
}

fn mount(fixture: &Fixture) -> Result<AssetStore, Box<dyn Error>> {
    let data_root = ClientDataRoot::new(fixture.data_root())?;
    Ok(AssetStore::mount(ArchiveCatalog::discover(
        data_root,
        Locale::EnUs,
    )?)?)
}
