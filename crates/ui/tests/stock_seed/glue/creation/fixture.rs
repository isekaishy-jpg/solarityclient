//! Small authored DBC graph with distinct race, class, and appearance orders.

use std::cell::RefCell;
use std::error::Error;
use std::rc::Rc;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_cpu::BlizzardRand;
use solarity_ui::UiCharacterCreationState;

use crate::support::{Fixture, FixtureFile};

/// Owns both the archive input and an independently inspectable random stream.
pub(super) struct CreationFixture {
    pub(super) state: UiCharacterCreationState,
    pub(super) random: Rc<RefCell<BlizzardRand>>,
    _archive: Fixture,
}

impl CreationFixture {
    /// Three races include one expansion gate; class rows differ from base order.
    pub(super) fn new() -> Result<Self, Box<dyn Error>> {
        let strings = b"\0Alliance\0Horde\0Race\0Class\0";
        let mut races = Vec::new();
        let mut sections = Vec::new();
        let mut hair = Vec::new();
        let mut facial = Vec::new();
        let mut combinations = Vec::new();
        for race in 1..=3 {
            let mut row = vec![0; 69];
            row[0] = race;
            row[2] = if race == 2 { 2 } else { 1 };
            row[6] = 16;
            row[11] = 16;
            row[14] = 16;
            row[68] = if race == 2 { 2 } else { 0 };
            races.push(row);
            for class in [8, 1, 6] {
                combinations.extend_from_slice(&[race as u8, class]);
            }
            for gender in 0..2 {
                for skin in 0..4 {
                    let flags = if skin == 3 { 0x05 } else { 0x11 };
                    for kind in [0, 4] {
                        if race == 3 && skin == 2 && kind == 4 {
                            continue;
                        }
                        section(&mut sections, race, gender, kind, 0, skin, flags);
                    }
                    for face in 0..4 {
                        if race == 3 && skin == 1 && face == 0 {
                            continue;
                        }
                        if race == 2 {
                            // Like the Gnome DK rows, exclusive skins and
                            // ordinary skins expose different eligible faces.
                            if skin == 3 && face >= 2 {
                                continue;
                            }
                            let flags = if skin == 3 || face >= 2 { 0x05 } else { 0x01 };
                            section(&mut sections, race, gender, 1, face, skin, flags);
                            continue;
                        }
                        section(&mut sections, race, gender, 1, face, skin, flags);
                    }
                }
                for style in 0..5 {
                    hair.push(vec![hair.len() as u32 + 1, race, gender, style, 1, 0]);
                    for color in 0..7 {
                        if race == 3 && style == 1 && color < 2 {
                            continue;
                        }
                        let flags = if color == 6 { 0x05 } else { 0x11 };
                        section(&mut sections, race, gender, 3, style, color, flags);
                    }
                }
                // Geometry-only features exercise stock's no-texture count path.
                for style in 0..3 {
                    facial.push(vec![race, gender, style, 1, 0, 0, 0, 0]);
                    // Race two mixes an untextured feature with two textured
                    // features whose supported color ranges differ.
                    if race == 2 && style > 0 {
                        for color in 0..7 {
                            if style == 1 && color < 2 {
                                continue;
                            }
                            let flags = if color == 6 { 0x05 } else { 0x11 };
                            section(&mut sections, race, gender, 2, style, color, flags);
                        }
                    }
                }
            }
        }
        let classes = [1, 8, 6].map(|class| {
            let mut row = vec![0; 60];
            row[0] = class;
            row[4] = 21;
            row[55] = 21;
            row[59] = if class == 6 { 2 } else { 0 };
            row
        });
        let templates = [1, 2].map(|id| {
            let mut row = vec![0; 14];
            row[0] = id;
            row[3] = 1 << id;
            row
        });
        let groups = [(1, 1), (2, 10)].map(|(id, name)| {
            let mut row = vec![0; 20];
            row[0] = id;
            row[1] = id;
            row[2] = name;
            row[3] = name;
            row
        });
        let data = [
            ("DBFilesClient\\ChrRaces.dbc", table(69, &races, strings)),
            (
                "DBFilesClient\\ChrClasses.dbc",
                table(60, &classes, strings),
            ),
            (
                "DBFilesClient\\CharBaseInfo.dbc",
                packed_table(&combinations),
            ),
            (
                "DBFilesClient\\FactionTemplate.dbc",
                table(14, &templates, strings),
            ),
            (
                "DBFilesClient\\FactionGroup.dbc",
                table(20, &groups, strings),
            ),
            (
                "DBFilesClient\\CharSections.dbc",
                table(10, &sections, b"\0"),
            ),
            ("DBFilesClient\\CharHairGeosets.dbc", table(6, &hair, b"\0")),
            (
                "DBFilesClient\\CharacterFacialHairStyles.dbc",
                table(8, &facial, b"\0"),
            ),
        ];
        let files = data
            .iter()
            .map(|(path, bytes)| FixtureFile { path, bytes })
            .collect::<Vec<_>>();
        let archive = Fixture::new(&files)?;
        let catalog =
            ArchiveCatalog::discover(ClientDataRoot::new(archive.data_root())?, Locale::EnUs)?;
        let mut store = AssetStore::mount(catalog)?;
        let random = Rc::new(RefCell::new(BlizzardRand::new(0x1234_5678)));
        let state = UiCharacterCreationState::load(&mut store, false, Rc::clone(&random))?;
        Ok(Self {
            state,
            random,
            _archive: archive,
        })
    }
}

/// Adds an exact section key without requiring renderer-only texture payloads.
fn section(
    rows: &mut Vec<Vec<u32>>,
    race: u32,
    gender: u32,
    kind: u32,
    style: u32,
    color: u32,
    flags: u32,
) {
    rows.push(vec![
        rows.len() as u32 + 1,
        race,
        gender,
        kind,
        0,
        0,
        0,
        flags,
        style,
        color,
    ]);
}

/// Serializes fixed-width DBC rows used by the real catalog boundary.
fn table(fields: u32, rows: &[Vec<u32>], strings: &[u8]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for word in [rows.len() as u32, fields, fields * 4, strings.len() as u32] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    for row in rows {
        assert_eq!(row.len(), fields as usize);
        for word in row {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
    }
    bytes.extend_from_slice(strings);
    bytes
}

/// CharBaseInfo has two byte columns, unlike the word-oriented DBC tables.
fn packed_table(rows: &[u8]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for word in [rows.len() as u32 / 2, 2, 2, 1] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend_from_slice(rows);
    bytes.push(0);
    bytes
}
