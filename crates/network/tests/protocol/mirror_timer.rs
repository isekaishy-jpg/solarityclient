//! Wire admission for the native 519A50 mirror-timer receiver.

use super::WorldServerPacket;
use crate::WorldMirrorTimerUpdate;

#[test]
fn mirror_timer_wire_layout_retains_signed_values_and_unknown_ids() {
    let mut body = Vec::new();
    for word in [1_u32, (-1_i32) as u32, i32::MAX as u32, (-10_i32) as u32] {
        body.extend_from_slice(&word.to_le_bytes());
    }
    body.push(255);
    body.extend_from_slice(&5384_u32.to_le_bytes());
    let packet = WorldServerPacket::new(0x1d9, body.clone());
    assert_eq!(packet.name(), Some("SMSG_START_MIRROR_TIMER"));
    assert_eq!(
        packet.mirror_timer(),
        Ok(Some(WorldMirrorTimerUpdate::Start {
            timer: 1,
            value: -1,
            maximum: i32::MAX,
            scale: -10,
            paused: 255,
            spell_id: 5384,
        }))
    );
    for length in 0..body.len() {
        assert!(
            WorldServerPacket::new(0x1d9, body[..length].to_vec())
                .mirror_timer()
                .is_err()
        );
    }
    body.push(0);
    assert!(WorldServerPacket::new(0x1d9, body).mirror_timer().is_err());
    for (opcode, body, expected) in [
        (
            0x1da,
            vec![255, 255, 255, 255, 2],
            WorldMirrorTimerUpdate::Pause {
                timer: u32::MAX,
                paused: 2,
            },
        ),
        (
            0x1db,
            vec![3, 0, 0, 0],
            WorldMirrorTimerUpdate::Stop { timer: 3 },
        ),
    ] {
        assert_eq!(
            WorldServerPacket::new(opcode, body.clone()).mirror_timer(),
            Ok(Some(expected))
        );
        for length in 0..body.len() {
            assert!(
                WorldServerPacket::new(opcode, body[..length].to_vec())
                    .mirror_timer()
                    .is_err()
            );
        }
        let mut trailing = body;
        trailing.push(0);
        assert!(
            WorldServerPacket::new(opcode, trailing)
                .mirror_timer()
                .is_err()
        );
    }
    assert_eq!(WorldServerPacket::new(0, vec![]).mirror_timer(), Ok(None));
}
