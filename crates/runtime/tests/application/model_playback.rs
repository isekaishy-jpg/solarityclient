//! Deterministic Model playback tests using archive-decoded sequence records.

use std::error::Error;
use std::io::Cursor;

use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedM2Model,
    Locale,
};
use solarity_rendering::M2BonePose;
use wow_m2::header::M2Header;
use wow_m2::skin::OldSkinHeader;
use wow_m2::{M2Model, M2Version, OldSkin};

use super::M2Playback;
use crate::random::CrtRand;
use crate::test_support::ClientFixture;

/// Two looping variations plus a terminal sequence with reverse/hold fallback rows.
fn playback_model() -> Result<(DecodedM2Model, AnimationDataCatalog), Box<dyn Error>> {
    playback_model_with_blend(400)
}

fn playback_model_with_blend(
    blend_ms: u32,
) -> Result<(DecodedM2Model, AnimationDataCatalog), Box<dyn Error>> {
    let mut source = M2Model {
        header: M2Header::new(M2Version::WotLK),
        name: Some("Playback".to_owned()),
        ..M2Model::default()
    };
    source.header.num_skin_profiles = Some(1);
    let mut cursor = Cursor::new(Vec::new());
    source.write(&mut cursor)?;
    let mut bytes = cursor.into_inner();
    let name = bytes.len() as u32;
    bytes.extend_from_slice(b"Playback\0");
    bytes[8..12].copy_from_slice(&9_u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&name.to_le_bytes());
    let sequences = bytes.len() as u32;
    for (id, variation, duration, flags, next, cycles) in [
        (0_u16, 0_u16, 1_000_u32, 0x20_u32, 1_u16, 2_u32),
        (0, 1, 1_000, 0x20, u16::MAX, 2),
        (7, 0, 600, 0x21, u16::MAX, 1),
    ] {
        let mut sequence = [0_u8; 64];
        sequence[0..2].copy_from_slice(&id.to_le_bytes());
        sequence[2..4].copy_from_slice(&variation.to_le_bytes());
        sequence[4..8].copy_from_slice(&duration.to_le_bytes());
        sequence[12..16].copy_from_slice(&flags.to_le_bytes());
        sequence[16..20].copy_from_slice(&16_384_u32.to_le_bytes());
        sequence[20..24].copy_from_slice(&cycles.to_le_bytes());
        sequence[24..28].copy_from_slice(&cycles.to_le_bytes());
        sequence[28..32].copy_from_slice(&blend_ms.to_le_bytes());
        sequence[60..62].copy_from_slice(&next.to_le_bytes());
        bytes.extend_from_slice(&sequence);
    }
    bytes[0x1c..0x20].copy_from_slice(&3_u32.to_le_bytes());
    bytes[0x20..0x24].copy_from_slice(&sequences.to_le_bytes());
    let bones = bytes.len() as u32;
    let mut bone = [0_u8; 88];
    bone[0..4].copy_from_slice(&(-1_i32).to_le_bytes());
    for offset in [8, 18, 38, 58] {
        bone[offset..offset + 2].copy_from_slice(&u16::MAX.to_le_bytes());
    }
    bytes.extend_from_slice(&bone);
    bytes[0x2c..0x30].copy_from_slice(&1_u32.to_le_bytes());
    bytes[0x30..0x34].copy_from_slice(&bones.to_le_bytes());
    append_playback_translation(&mut bytes, bones as usize + 16);
    let skin = OldSkin {
        header: OldSkinHeader::new(),
        indices: Vec::new(),
        triangles: Vec::new(),
        bone_indices: Vec::new(),
        submeshes: Vec::new(),
        batches: Vec::new(),
    };
    let mut skin_bytes = Cursor::new(Vec::new());
    skin.write(&mut skin_bytes)?;
    let mut dbc = b"WDBC".to_vec();
    for value in [2_u32, 8, 32, 1] {
        dbc.extend_from_slice(&value.to_le_bytes());
    }
    for (id, flags) in [(5_u32, 0x20_u32), (6, 0x10)] {
        for value in [id, 0, 0, 0, flags, 7, id, 0] {
            dbc.extend_from_slice(&value.to_le_bytes());
        }
    }
    dbc.push(0);
    let fixture = ClientFixture::with_common_files(&[
        ("Solarity\\Playback.m2", &bytes),
        ("Solarity\\Playback00.skin", skin_bytes.get_ref()),
        ("DBFilesClient\\AnimationData.dbc", &dbc),
    ])?;
    let archive =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(archive)?;
    Ok((
        DecodedM2Model::load(&mut store, &AssetPath::new("Solarity\\Playback.m2")?)?,
        AnimationDataCatalog::load(&mut store)?,
    ))
}

/// Sequence-specific root positions expose whether an automatic blend survives.
fn append_playback_translation(bytes: &mut Vec<u8>, track: usize) {
    let timestamps = bytes.len() as u32;
    for time in [0_u32, 1_000] {
        bytes.extend_from_slice(&time.to_le_bytes());
    }
    let mut values = Vec::new();
    for base in [0.0_f32, 100.0, 500.0] {
        values.push(bytes.len() as u32);
        for value in [base, 0.0, 0.0, base + 100.0, 0.0, 0.0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    let time_refs = bytes.len() as u32;
    for _ in 0..3 {
        bytes.extend_from_slice(&2_u32.to_le_bytes());
        bytes.extend_from_slice(&timestamps.to_le_bytes());
    }
    let value_refs = bytes.len() as u32;
    for offset in values {
        bytes.extend_from_slice(&2_u32.to_le_bytes());
        bytes.extend_from_slice(&offset.to_le_bytes());
    }
    bytes[track..track + 2].copy_from_slice(&1_u16.to_le_bytes());
    for (offset, value) in [(4, 3), (8, time_refs), (12, 3), (16, value_refs)] {
        bytes[track + offset..track + offset + 4].copy_from_slice(&value.to_le_bytes());
    }
}

/// Equal requests consume separate variation/cycle rolls and establish fresh timers.
#[test]
fn model_requests_restart_seek_and_preserve_the_native_random_stream() -> Result<(), Box<dyn Error>>
{
    let (model, catalog) = playback_model()?;
    let mut random = CrtRand::new();
    let mut playback = M2Playback::new(&model, 0, &mut random)?.ok_or("missing playback")?;
    playback.apply_model_sequence(&model, &catalog, 0, 0, &mut random)?;
    let first = playback.clock(&model, 250.0, 250.0, &mut random)?.clock;
    assert_eq!((first.sequence(), first.animation_time_ms()), (1, 249.0));
    playback.event_window(250.0, 250.0);
    playback.apply_model_sequence(&model, &catalog, 0, 0, &mut random)?;
    assert_eq!(
        playback
            .clock(&model, 251.0, 251.0, &mut random)?
            .clock
            .animation_time_ms(),
        0.0
    );
    playback.apply_model_sequence(&model, &catalog, 0, 450, &mut random)?;
    let seek = playback.clock(&model, 252.0, 252.0, &mut random)?.clock;
    assert_eq!((seek.sequence(), seek.animation_time_ms()), (0, 450.0));
    let mut next = random;
    assert_eq!(next.next_u15(), 29_358);
    playback.apply_model_sequence(&model, &catalog, u32::MAX, 0, &mut random)?;
    assert_eq!(
        playback
            .clock(&model, 253.0, 253.0, &mut random)?
            .clock
            .animation_time_ms(),
        451.0
    );
    assert_eq!(random.next_u15(), 29_358);
    playback.apply_model_sequence(&model, &catalog, 6, 200, &mut random)?;
    let reverse = playback.clock(&model, 254.0, 254.0, &mut random)?.clock;
    assert_eq!(
        (reverse.sequence(), reverse.animation_time_ms()),
        (2, 400.0)
    );
    assert_eq!(
        playback
            .clock(&model, 900.0, 900.0, &mut random)?
            .clock
            .animation_time_ms(),
        0.0
    );
    playback.apply_model_sequence(&model, &catalog, 5, 123, &mut random)?;
    assert_eq!(
        playback
            .clock(&model, 901.0, 901.0, &mut random)?
            .clock
            .animation_time_ms(),
        600.0
    );
    Ok(())
}

/// 0x00832260/0x00831FC0 retain overdue time and each intervening variation roll.
#[test]
fn model_variation_callbacks_preserve_long_frame_remainder_and_event_tails()
-> Result<(), Box<dyn Error>> {
    let (model, catalog) = playback_model()?;
    let mut random = CrtRand::new();
    let mut playback = M2Playback::new(&model, 0, &mut random)?.ok_or("missing playback")?;
    playback.apply_model_sequence(&model, &catalog, 0, 0, &mut random)?;
    playback.clock(&model, 250.0, 250.0, &mut random)?;
    playback.event_window(250.0, 250.0);
    let advance = playback.clock(&model, 2_200.0, 2_200.0, &mut random)?;
    assert_eq!(advance.expired_variations.len(), 2);
    assert_eq!(
        (advance.clock.sequence(), advance.clock.animation_time_ms()),
        (0, 201.0)
    );
    assert_eq!(random.next_u15(), 29_358);
    // Both callbacks run at scene tick 2200. The first retains sequence 1's
    // original timer with full weight, so the second must not overwrite it.
    let pose = M2BonePose::compose(model.animations(), advance.clock)?;
    assert!((pose.transforms()[0].w_axis.x - 119.9).abs() < 0.0001);
    Ok(())
}

/// The incoming duration drives the fade, and a later Lua request clears it.
#[test]
fn model_variations_blend_then_explicit_sequence_calls_replace_the_timer()
-> Result<(), Box<dyn Error>> {
    let (model, catalog) = playback_model()?;
    let mut random = CrtRand::new();
    let mut playback = M2Playback::new(&model, 0, &mut random)?.ok_or("missing playback")?;
    playback.apply_model_sequence(&model, &catalog, 0, 0, &mut random)?;
    for tick in [250.0, 1_000.0, 1_400.0] {
        playback.clock(&model, tick, tick, &mut random)?;
        playback.event_window(tick, tick);
    }
    let start = playback.clock(&model, 1_999.0, 1_999.0, &mut random)?.clock;
    assert_eq!(start.sequence(), 0);
    let pose = M2BonePose::compose(model.animations(), start)?;
    assert!((pose.transforms()[0].w_axis.x - 199.9).abs() < 0.0001);
    playback.event_window(1_999.0, 1_999.0);
    let halfway = playback.clock(&model, 2_199.0, 2_199.0, &mut random)?.clock;
    let pose = M2BonePose::compose(model.animations(), halfway)?;
    assert!((pose.transforms()[0].w_axis.x - 69.95).abs() < 0.0001);
    playback.event_window(2_199.0, 2_199.0);
    playback.apply_model_sequence(&model, &catalog, 0, 500, &mut random)?;
    let explicit = playback.clock(&model, 2_200.0, 2_200.0, &mut random)?.clock;
    assert_eq!(explicit.sequence(), 1);
    let pose = M2BonePose::compose(model.animations(), explicit)?;
    assert!((pose.transforms()[0].w_axis.x - 150.0).abs() < 0.0001);
    Ok(())
}

/// 0x00826C40 preserves a secondary only while its weight is strictly above 0.5.
#[test]
fn model_variation_blend_replaces_the_secondary_at_exactly_half_weight()
-> Result<(), Box<dyn Error>> {
    for (duration, expected_x) in [(1_998, 199.9_f32), (1_999, 199.8)] {
        let (model, catalog) = playback_model_with_blend(duration)?;
        let mut random = CrtRand::new();
        let mut playback = M2Playback::new(&model, 0, &mut random)?.ok_or("missing playback")?;
        playback.apply_model_sequence(&model, &catalog, 0, 0, &mut random)?;
        for tick in [250.0, 1_000.0] {
            playback.clock(&model, tick, tick, &mut random)?;
            playback.event_window(tick, tick);
        }
        let clock = playback.clock(&model, 1_999.0, 1_999.0, &mut random)?.clock;
        let pose = M2BonePose::compose(model.animations(), clock)?;
        // The newly installed blend has full weight; a retained one still has
        // just over half, so its output includes the incoming sequence at zero.
        let expected_x = if duration == 1_999 {
            let fraction = 1_000.0_f32 / 1_999.0;
            expected_x * (3.0 - 2.0 * fraction) * fraction * fraction
        } else {
            expected_x
        };
        assert!((pose.transforms()[0].w_axis.x - expected_x).abs() < 0.0001);
    }
    Ok(())
}
