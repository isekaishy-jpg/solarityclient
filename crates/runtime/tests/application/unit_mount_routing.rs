//! The mounted adapter is compared with original 7385C0 submissions.

use super::*;

fn add_upper_bone(bytes: &mut Vec<u8>) {
    let old = u32::from_le_bytes([bytes[0x30], bytes[0x31], bytes[0x32], bytes[0x33]]) as usize;
    let mut root = bytes[old..old + 88].to_vec();
    root[..4].copy_from_slice(&(-1_i32).to_le_bytes());
    let mut upper = root.clone();
    upper[..4].copy_from_slice(&4_i32.to_le_bytes());
    upper[8..10].copy_from_slice(&0_i16.to_le_bytes());
    let offset = bytes.len() as u32;
    bytes.extend(root);
    bytes.extend(upper);
    bytes[0x2c..0x30].copy_from_slice(&2_u32.to_le_bytes());
    bytes[0x30..0x34].copy_from_slice(&offset.to_le_bytes());
    let keys = u32::from_le_bytes([bytes[0x38], bytes[0x39], bytes[0x3a], bytes[0x3b]]) as usize;
    bytes[keys + 8..keys + 10].copy_from_slice(&1_i16.to_le_bytes());
}

#[test]
fn mount_and_rider_slots_match_native_ordinary_request_dispatch() -> Result<(), Box<dyn Error>> {
    let ids = (0..506).collect::<Vec<u16>>();
    let mut mounted = input(0);
    mounted.mounted = true;
    let fixture = owner_with_model_metadata(&ids, mounted, |_, _, _| {}, add_upper_bone)?;
    let mut checked = 0;
    for line in include_str!("../fixtures/unit_mount_owner_native.txt").lines() {
        if line.starts_with('#') {
            continue;
        }
        let row = line
            .split_whitespace()
            .map(str::parse::<i32>)
            .collect::<Result<Vec<_>, _>>()?;
        // This test exercises ordinary Unit request admission. The oracle also
        // records the body/mount callback masks and finished-record guards.
        if row[0] != 0x70 || row[2] != 0 || row[3] != 0 {
            continue;
        }
        let owner = UnitAnimationBehavior::new(
            fixture.identity,
            Arc::clone(&fixture.model),
            Arc::clone(&fixture.animations),
            mounted,
            100,
        );
        let mut random = CrtRand::new();
        let mut body = owner.playback.borrow_mut();
        owner.select(
            &mut body,
            91.into(),
            mounted,
            100,
            M2SequenceStartPhase::DuringSceneUpdate,
            &mut random,
        )?;
        let mut mount =
            M2Playback::default_sequence(&fixture.model, &fixture.animations, 100, &mut random)?;
        mount.apply_resolved_model_sequence_variation(
            &fixture.model,
            5,
            None,
            M2ModelAnimationMode::Forward,
            1.,
            0,
            100,
            M2SequenceStartPhase::DuringSceneUpdate,
            true,
            &mut random,
        )?;
        let request = row[4] as u16;
        let state = UnitAnimationInput {
            alive: row[1] == 0,
            ..mounted
        };
        let before = random;
        if let Some(resolved) = owner.commit_mount_sequence(
            &fixture.model,
            &mut mount,
            request.into(),
            state,
            101,
            M2SequenceStartPhase::BeforeSceneUpdate,
            &mut random,
        )? {
            owner.commit_mounted_body(
                &mut body,
                resolved,
                state,
                101,
                M2SequenceStartPhase::BeforeSceneUpdate,
                &mut random,
            )?;
        }
        assert_eq!(
            i32::from(mount.animation_id),
            if row[5] == -1 { 5 } else { row[5] },
            "{line}"
        );
        assert_eq!(
            body.bone_playback(4)
                .map_or(-1, |slot| i32::from(slot.animation_id)),
            row[6],
            "{line}"
        );
        assert_eq!(
            i32::from(body.animation_id),
            if row[7] == -1 { 91 } else { row[7] },
            "{line}"
        );
        if row[1] == 1 && row[5..].iter().all(|&value| value == -1) {
            assert_eq!(
                random, before,
                "dead request rejected before any model roll: {line}"
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 56);
    Ok(())
}

#[test]
fn mount_death_resolution_precedes_the_riders_model_fallback() -> Result<(), Box<dyn Error>> {
    let mut mounted = input(0);
    mounted.mounted = true;
    let owner = owner_with_input(POSES, mounted)?;
    let mount = owner_with_input(&[0, 1, 6], input(0))?;
    let mut random = CrtRand::new();
    let clock = bind_mount(&owner, &mount, 100, &mut random)?;
    mounted.alive = false;
    owner.set_input(mounted);
    owner.synchronize(200, &mut random)?;
    assert_eq!(clock.borrow().animation_id, 1);
    assert_eq!(
        owner.playback.borrow().animation_id,
        1,
        "body has 466, but receives the mount's resolved death 1"
    );
    advance_mount(
        &owner,
        &mount.model,
        &mut clock.borrow_mut(),
        1500,
        &mut random,
    )?;
    assert_eq!(clock.borrow().animation_id, 6);
    assert_eq!(
        owner.playback.borrow().animation_id,
        1,
        "direct corpse completion belongs to the emitting model"
    );
    Ok(())
}

#[test]
fn mount_corpse_request_uses_mount_ordinal_only_when_body_is_in_death_family()
-> Result<(), Box<dyn Error>> {
    for body_dying in [false, true] {
        let mut mounted = input(0);
        mounted.mounted = true;
        let owner = owner_with_input(POSES, mounted)?;
        let mount = owner_with_input(&[0, 1, 1, 6, 6], input(0))?;
        let mut random = CrtRand::new();
        let clock = bind_mount(&owner, &mount, 100, &mut random)?;
        clock.borrow_mut().apply_resolved_model_sequence_variation(
            &mount.model,
            1,
            Some(1),
            M2ModelAnimationMode::Forward,
            1.,
            0,
            200,
            M2SequenceStartPhase::DuringSceneUpdate,
            true,
            &mut random,
        )?;
        if body_dying {
            owner.select(
                &mut owner.playback.borrow_mut(),
                1.into(),
                mounted,
                200,
                M2SequenceStartPhase::DuringSceneUpdate,
                &mut random,
            )?;
        }
        let request = owner.mount_sequence_request(
            6,
            &owner.playback.borrow(),
            &mount.model,
            &clock.borrow(),
        );
        assert_eq!(request.variation, body_dying.then_some(1));
        let before = random;
        owner.commit_mount_sequence(
            &mount.model,
            &mut clock.borrow_mut(),
            request,
            mounted,
            201,
            M2SequenceStartPhase::BeforeSceneUpdate,
            &mut random,
        )?;
        assert_eq!(clock.borrow().animation_id, 6);
        let mut expected = before;
        for _ in 0..if body_dying { 1 } else { 2 } {
            let _roll = expected.next_u15();
        }
        assert_eq!(random, expected);
    }
    Ok(())
}
