use super::WorldServerPacket;
use crate::WorldBattlefieldStatus;

#[test]
fn battlefield_status_validates_native_branches_and_ignores_trailing_bytes() {
    for queue in [0u32, 1, 2, u32::MAX] {
        for (status, required) in [(0u32, 23), (1, 31), (2, 39), (3, 44), (99, 23)] {
            let mut body = vec![0; required];
            body[..4].copy_from_slice(&queue.to_le_bytes());
            body[8] = 1; // High GUID word alone is nonzero.
            body[19..23].copy_from_slice(&status.to_le_bytes());
            if matches!(status, 2 | 3) {
                body[23..27].copy_from_slice(&u32::MAX.to_le_bytes());
            }
            let expected = if queue < 2 {
                WorldBattlefieldStatus::Status {
                    queue,
                    status,
                    map: matches!(status, 2 | 3).then_some(u32::MAX),
                }
            } else {
                WorldBattlefieldStatus::Ignored
            };
            for length in 0..required {
                let result =
                    WorldServerPacket::new(0x2d4, body[..length].to_vec()).battlefield_status();
                if queue >= 2 && length >= 4 {
                    assert_eq!(result, Ok(Some(expected)));
                } else {
                    assert!(
                        result.is_err(),
                        "queue {queue}, status {status}, length {length}"
                    );
                }
            }
            for extra in [false, true] {
                if extra {
                    body.extend_from_slice(&[1, 2, 3]);
                }
                let packet = WorldServerPacket::new(0x2d4, body.clone());
                assert_eq!(packet.name(), Some("SMSG_BATTLEFIELD_STATUS"));
                assert_eq!(packet.battlefield_status(), Ok(Some(expected)));
                assert_eq!(
                    WorldServerPacket::new(0x2d3, body.clone()).battlefield_status(),
                    Ok(None)
                );
            }
        }
    }
    for queue in 0u32..2 {
        let mut body = vec![0; 12];
        body[..4].copy_from_slice(&queue.to_le_bytes());
        for extra in [false, true] {
            if extra {
                body.push(255);
            }
            assert_eq!(
                WorldServerPacket::new(0x2d4, body.clone()).battlefield_status(),
                Ok(Some(WorldBattlefieldStatus::Cleared(queue)))
            );
        }
    }
}
