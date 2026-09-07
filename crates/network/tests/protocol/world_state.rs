//! Native 52693A/5269E5 bodies exercised through the packet dispatcher.

use super::WorldServerPacket;
use crate::WorldStateUpdate;

#[test]
fn world_state_packets_preserve_order_bits_and_declared_counts()
-> Result<(), Box<dyn std::error::Error>> {
    let mut body = Vec::new();
    for word in [571u32, 4197, 4200] {
        body.extend(word.to_le_bytes());
    }
    body.extend(3u16.to_le_bytes());
    for word in [77u32, 1, 88, u32::MAX, 77, 2] {
        body.extend(word.to_le_bytes());
    }
    let packet = WorldServerPacket::new(0x2c2, body.clone());
    assert_eq!(packet.name(), Some("SMSG_INIT_WORLD_STATES"));
    assert_eq!(
        packet.world_state_update()?,
        Some(WorldStateUpdate::Initialize {
            location: [571, 4197, 4200],
            values: vec![(77, 1), (88, u32::MAX), (77, 2)],
        })
    );
    for length in 0..body.len() {
        assert!(
            WorldServerPacket::new(0x2c2, body[..length].to_vec())
                .world_state_update()
                .is_err()
        );
    }
    body.push(0);
    assert!(
        WorldServerPacket::new(0x2c2, body)
            .world_state_update()
            .is_err()
    );
    let update: Vec<_> = [77u32, 0x8000_0000]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    let packet = WorldServerPacket::new(0x2c3, update.clone());
    assert_eq!(packet.name(), Some("SMSG_UPDATE_WORLD_STATE"));
    assert_eq!(
        packet.world_state_update()?,
        Some(WorldStateUpdate::Value {
            field: 77,
            value: 0x8000_0000
        })
    );
    for length in 0..8 {
        assert!(
            WorldServerPacket::new(0x2c3, update[..length].to_vec())
                .world_state_update()
                .is_err()
        );
    }
    assert!(
        WorldServerPacket::new(0x2c3, vec![0; 9])
            .world_state_update()
            .is_err()
    );
    assert_eq!(
        WorldServerPacket::new(0x42, vec![]).world_state_update()?,
        None
    );
    Ok(())
}
