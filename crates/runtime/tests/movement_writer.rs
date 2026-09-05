//! Native movement wire images and bounded runtime writer ownership.

#[path = "support/transfer_authentication.rs"]
mod transfer_authentication;
#[path = "support/transfer_world_server.rs"]
mod transfer_world_server;

use solarity_network::{
    ObjectMovementContext, ObjectMovementFall, ObjectMovementTransport, WorldMovementEncodeError,
    WorldMovementField, WorldMovementKind, WorldMovementMessage,
};
use solarity_runtime::RuntimeGameplayCoordinator;

use transfer_world_server::{TestError, WorldServer};

/// Every ordinary movement envelope shares its native snapshot layout with
/// time sync and a map ACK while retaining application admission order.
#[test]
fn movement_events_share_native_wire_order_with_liveness_and_worldport_ack() -> Result<(), TestError>
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                let (server, session) = WorldServer::connect().await?;
                let mut gameplay = RuntimeGameplayCoordinator::new();
                gameplay.begin(&tokio::runtime::Handle::current(), session, Vec::new())?;
                let responses = server
                    .exchange_raw(
                        vec![(0x390, 77_u32.to_le_bytes().to_vec())],
                        EVENTS.len() + 2,
                    )
                    .await?;
                for (kind, _) in EVENTS {
                    let message = full_message(kind, 0xFFFF_FFF0)?;
                    while !gameplay.send_movement(message)? {
                        tokio::task::yield_now().await;
                    }
                }
                while !gameplay.acknowledge_world_transfer()? {
                    tokio::task::yield_now().await;
                }
                let responses = responses.await??;
                let mut index = 0;
                let mut time_syncs = 0;
                let mut acknowledgements = 0;
                for (opcode, body) in responses {
                    if opcode == 0x391 {
                        assert_eq!(body.len(), 8);
                        assert_eq!(&body[..4], &77_u32.to_le_bytes());
                        time_syncs += 1;
                    } else if opcode == 0xDC {
                        assert_eq!(index, EVENTS.len());
                        assert!(body.is_empty());
                        acknowledgements += 1;
                    } else {
                        assert_eq!(opcode, EVENTS[index].1);
                        assert_eq!(body, FULL_BODY);
                        index += 1;
                    }
                }
                assert_eq!(index, EVENTS.len());
                assert_eq!(time_syncs, 1);
                assert_eq!(acknowledgements, 1);
                // A subsequent reply must also remain decodable through the
                // independent message library after all custom movement writes.
                let next = server
                    .exchange(vec![(0x390, 78_u32.to_le_bytes().to_vec())], 1)
                    .await?
                    .await??;
                assert!(matches!(
                    &next[0],
                    wow_world_messages::wrath::opcodes::ClientOpcodeMessage::CMSG_TIME_SYNC_RESP(response)
                        if response.time_sync == 78
                ));
                Ok::<(), TestError>(())
            })
            .await?
        })
}

/// Saturation is deterministic before yielding the current-thread executor.
/// The rejected event remains with its caller and is later emitted exactly once.
#[test]
fn movement_backpressure_preserves_the_rejected_snapshot_and_following_ack() -> Result<(), TestError>
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                let (server, session) = WorldServer::connect().await?;
                let mut gameplay = RuntimeGameplayCoordinator::new();
                gameplay.begin(&tokio::runtime::Handle::current(), session, Vec::new())?;
                let mut accepted = 0_u32;
                let rejected = loop {
                    assert!(accepted < 256, "movement admission lost its bounded queue");
                    let message = full_message(WorldMovementKind::Heartbeat, accepted)?;
                    if !gameplay.send_movement(message)? {
                        break message;
                    }
                    accepted += 1;
                };
                assert!(accepted > 0);
                assert!(!gameplay.send_movement(rejected)?);
                assert!(!gameplay.acknowledge_world_transfer()?);
                let responses = server
                    .exchange_raw(Vec::new(), accepted as usize + 2)
                    .await?;
                while !gameplay.send_movement(rejected)? {
                    tokio::task::yield_now().await;
                }
                while !gameplay.acknowledge_world_transfer()? {
                    tokio::task::yield_now().await;
                }
                let responses = responses.await??;
                for (index, (opcode, body)) in responses[..=accepted as usize].iter().enumerate() {
                    assert_eq!(*opcode, 0xEE);
                    let mut expected = FULL_BODY;
                    // Packed GUID 9 + flags 6 precede the native event timestamp.
                    expected[15..19].copy_from_slice(&(index as u32).to_le_bytes());
                    assert_eq!(*body, expected);
                }
                assert_eq!(responses[accepted as usize + 1], (0xDC, Vec::new()));
                gameplay.disconnect();
                assert!(gameplay.send_movement(rejected).is_err());
                Ok::<(), TestError>(())
            })
            .await?
        })
}

/// A zero GUID still has a one-byte packed envelope. Flags that do not gate
/// a section alone must not introduce it, even after a previous full packet.
#[test]
fn minimal_movement_packet_preserves_absent_sections_and_integer_fall_time() -> Result<(), TestError>
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                let (server, session) = WorldServer::connect().await?;
                let (_reader, mut writer) = session.split();
                let response = server.exchange_raw(Vec::new(), 2).await?;
                writer
                    .send_movement(&full_message(WorldMovementKind::Heartbeat, 0xFFFF_FFF0)?)
                    .await?;
                let context = ObjectMovementContext {
                    timestamp_ms: 0,
                    transport: None,
                    pitch_radians: None,
                    fall_time_ms: u32::MAX,
                    falling: None,
                    spline_elevation: None,
                };
                let message = WorldMovementMessage::new(
                    WorldMovementKind::Stop,
                    0,
                    0x0400_0000_2000,
                    [0.0; 3],
                    0.0,
                    context,
                )?;
                writer.send_movement(&message).await?;
                let response = response.await??;
                assert_eq!(response[0], (0xEE, FULL_BODY.to_vec()));
                assert_eq!(
                    response[1],
                    (
                        0xB7,
                        vec![
                            0, // Packed zero GUID.
                            0, 0x20, 0, 0, 0,
                            4, // FALLING_FAR and INTERPOLATED, no optional sections.
                            0, 0, 0, 0, // Timestamp.
                            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // XYZ/facing.
                            0xFF, 0xFF, 0xFF,
                            0xFF, // Integer fall time, not a numeric f32 conversion.
                        ]
                    )
                );
                Ok::<(), TestError>(())
            })
            .await?
        })
}

/// Invalid field combinations cannot form a message or reach the cipher owner.
#[test]
fn movement_packet_rejects_inconsistent_optional_sections_and_overwide_flags() {
    for (bit, field) in [
        (0x200, WorldMovementField::Transport),
        (0x0400_0000_0000, WorldMovementField::TransportInterpolation),
        (0x0020_0000_0000, WorldMovementField::Pitch),
        (0x1000, WorldMovementField::Falling),
        (0x0400_0000, WorldMovementField::SplineElevation),
    ] {
        let result = WorldMovementMessage::new(
            WorldMovementKind::Heartbeat,
            8,
            FULL_FLAGS & !bit,
            [0.0; 3],
            0.0,
            full_context(0),
        );
        assert_eq!(
            result,
            Err(WorldMovementEncodeError::FieldPresence { field })
        );
        let mut missing = full_context(0);
        match field {
            WorldMovementField::Transport => missing.transport = None,
            WorldMovementField::TransportInterpolation => {
                if let Some(transport) = missing.transport.as_mut() {
                    transport.interpolated_time_ms = None;
                }
            }
            WorldMovementField::Pitch => missing.pitch_radians = None,
            WorldMovementField::Falling => missing.falling = None,
            WorldMovementField::SplineElevation => missing.spline_elevation = None,
        }
        assert_eq!(
            WorldMovementMessage::new(
                WorldMovementKind::Heartbeat,
                8,
                FULL_FLAGS,
                [0.0; 3],
                0.0,
                missing
            ),
            Err(WorldMovementEncodeError::FieldPresence { field })
        );
    }
    let flags = FULL_FLAGS | (1_u64 << 48);
    assert_eq!(
        WorldMovementMessage::new(
            WorldMovementKind::Heartbeat,
            8,
            flags,
            [0.0; 3],
            0.0,
            full_context(0)
        ),
        Err(WorldMovementEncodeError::FlagWidth { flags })
    );
}

const FULL_FLAGS: u64 = 0x1420_0400_1201;

/// Native-complete context with values chosen to expose byte-order and type errors.
fn full_context(timestamp_ms: u32) -> ObjectMovementContext {
    ObjectMovementContext {
        timestamp_ms,
        transport: Some(ObjectMovementTransport {
            guid: 0x1112_1314_1516_1718,
            position: [1.0, 2.0, 3.0],
            orientation: -0.5,
            time_ms: 0x1122_3344,
            seat: -1,
            interpolated_time_ms: Some(0x5566_7788),
        }),
        pitch_radians: Some(-0.25),
        fall_time_ms: 0x8000_0001,
        falling: Some(ObjectMovementFall {
            vertical_speed: 7.95,
            direction_cos: 0.8,
            direction_sin: -0.6,
            horizontal_speed: 4.5,
        }),
        spline_elevation: Some(1.25),
    }
}

/// Creates the fixture event while keeping the expected wire image independent.
fn full_message(
    kind: WorldMovementKind,
    timestamp_ms: u32,
) -> Result<WorldMovementMessage, WorldMovementEncodeError> {
    WorldMovementMessage::new(
        kind,
        0x0102_0304_0506_0708,
        FULL_FLAGS,
        [-1.5, 2.25, -0.0],
        0.75,
        full_context(timestamp_ms),
    )
}

// 0x006EF860's ordinary event cases, 0x006F09F0's heartbeat, and the
// landing/attachment dispatchers 0x0073D4A0 and 0x006EB0B0.
const EVENTS: [(WorldMovementKind, u32); 25] = [
    (WorldMovementKind::StartForward, 0xB5),
    (WorldMovementKind::StartBackward, 0xB6),
    (WorldMovementKind::Stop, 0xB7),
    (WorldMovementKind::StartStrafeLeft, 0xB8),
    (WorldMovementKind::StartStrafeRight, 0xB9),
    (WorldMovementKind::StopStrafe, 0xBA),
    (WorldMovementKind::Jump, 0xBB),
    (WorldMovementKind::StartTurnLeft, 0xBC),
    (WorldMovementKind::StartTurnRight, 0xBD),
    (WorldMovementKind::StopTurn, 0xBE),
    (WorldMovementKind::StartPitchUp, 0xBF),
    (WorldMovementKind::StartPitchDown, 0xC0),
    (WorldMovementKind::StopPitch, 0xC1),
    (WorldMovementKind::SetRunMode, 0xC2),
    (WorldMovementKind::SetWalkMode, 0xC3),
    (WorldMovementKind::FallLand, 0xC9),
    (WorldMovementKind::StartSwim, 0xCA),
    (WorldMovementKind::StopSwim, 0xCB),
    (WorldMovementKind::SetFacing, 0xDA),
    (WorldMovementKind::SetPitch, 0xDB),
    (WorldMovementKind::Heartbeat, 0xEE),
    (WorldMovementKind::StartAscend, 0x359),
    (WorldMovementKind::StopAscend, 0x35A),
    (WorldMovementKind::ChangeTransport, 0x38D),
    (WorldMovementKind::StartDescend, 0x3A7),
];

// Golden body follows original 0x0071EF80 packed GUID + 0x004F4ED0 scalar
// order. Both GUIDs use every octet, exercising the exact 97-byte maximum.
const FULL_BODY: [u8; 97] = [
    0xFF, 0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, 0x01, 0x12, 0x00, 0x04, 0x20, 0x14, 0xF0,
    0xFF, 0xFF, 0xFF, 0x00, 0x00, 0xC0, 0xBF, 0x00, 0x00, 0x10, 0x40, 0x00, 0x00, 0x00, 0x80, 0x00,
    0x00, 0x40, 0x3F, 0xFF, 0x18, 0x17, 0x16, 0x15, 0x14, 0x13, 0x12, 0x11, 0x00, 0x00, 0x80, 0x3F,
    0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x40, 0x40, 0x00, 0x00, 0x00, 0xBF, 0x44, 0x33, 0x22, 0x11,
    0xFF, 0x88, 0x77, 0x66, 0x55, 0x00, 0x00, 0x80, 0xBE, 0x01, 0x00, 0x00, 0x80, 0x66, 0x66, 0xFE,
    0x40, 0xCD, 0xCC, 0x4C, 0x3F, 0x9A, 0x99, 0x19, 0xBF, 0x00, 0x00, 0x90, 0x40, 0x00, 0x00, 0xA0,
    0x3F,
];
