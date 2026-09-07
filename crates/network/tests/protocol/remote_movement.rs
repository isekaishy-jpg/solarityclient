//! Independent server bodies exercise every MovementInfo presence combination.

use super::RemoteMovement;

#[test]
fn conditional_fields_and_every_truncation_are_bounded() -> Result<(), Box<dyn std::error::Error>> {
    for shape in 0..64 {
        let transport = shape & 1 != 0;
        let interpolation = shape & 2 != 0;
        let pitch = shape & 4 != 0;
        let falling = shape & 8 != 0;
        let elevation = shape & 16 != 0;
        let spline = shape & 32 != 0;
        let flags = (u64::from(transport) * 0x200)
            | (u64::from(interpolation) * 0x0400_0000_0000)
            | (u64::from(pitch) * 0x0020_0000_0000)
            | (u64::from(falling) * 0x1000)
            | (u64::from(elevation) * 0x0400_0000)
            | (u64::from(spline) * 0x0800_0000);
        let mut body = vec![0x81, 0x42, 0xf1];
        body.extend_from_slice(&(flags as u32).to_le_bytes());
        body.extend_from_slice(&((flags >> 32) as u16).to_le_bytes());
        body.extend_from_slice(&u32::MAX.to_le_bytes());
        for value in [1_f32, -2., 3., 0.5] {
            body.extend_from_slice(&value.to_le_bytes());
        }
        if transport {
            body.extend_from_slice(&[0x02, 0x77]);
            for value in [4_f32, 5., 6., -0.25] {
                body.extend_from_slice(&value.to_le_bytes());
            }
            body.extend_from_slice(&20_u32.to_le_bytes());
            body.push(0xff);
            if interpolation {
                body.extend_from_slice(&30_u32.to_le_bytes());
            }
        }
        if pitch {
            body.extend_from_slice(&0.75_f32.to_le_bytes());
        }
        body.extend_from_slice(&40_u32.to_le_bytes());
        if falling {
            for value in [-7_f32, 0.6, 0.8, 9.] {
                body.extend_from_slice(&value.to_le_bytes());
            }
        }
        if elevation {
            body.extend_from_slice(&10_f32.to_le_bytes());
        }
        let decoded = RemoteMovement::decode(0xee, &body)?.ok_or("heartbeat was not decoded")?;
        assert_eq!(decoded.guid, 0xf100_0000_0000_0042);
        assert_eq!(decoded.flags, flags);
        assert_eq!(decoded.position, [1., -2., 3.]);
        assert_eq!(decoded.orientation, 0.5);
        assert_eq!(decoded.context.timestamp_ms, u32::MAX);
        assert_eq!(decoded.context.transport.is_some(), transport);
        if let Some(attachment) = decoded.context.transport {
            assert_eq!(attachment.guid, 0x7700);
            assert_eq!(attachment.position, [4., 5., 6.]);
            assert_eq!(attachment.orientation, -0.25);
            assert_eq!(attachment.seat, -1);
            assert_eq!(attachment.time_ms, 20);
            assert_eq!(attachment.interpolated_time_ms, interpolation.then_some(30));
        }
        assert_eq!(decoded.context.pitch_radians, pitch.then_some(0.75));
        assert_eq!(decoded.context.fall_time_ms, 40);
        assert_eq!(decoded.context.falling.is_some(), falling);
        if let Some(launch) = decoded.context.falling {
            assert_eq!(launch.vertical_speed, -7.);
            assert_eq!([launch.direction_cos, launch.direction_sin], [0.6, 0.8]);
            assert_eq!(launch.horizontal_speed, 9.);
        }
        assert_eq!(decoded.context.spline_elevation, elevation.then_some(10.));
        for end in 0..body.len() {
            assert!(
                RemoteMovement::decode(0xee, &body[..end]).is_err(),
                "shape {shape}, length {end}"
            );
        }
        body.push(0);
        assert!(RemoteMovement::decode(0xee, &body).is_err());
    }
    Ok(())
}

#[test]
fn dispatch_admits_only_the_ordinary_server_envelope() -> Result<(), Box<dyn std::error::Error>> {
    let opcodes = [
        0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xbb, 0xbc, 0xbd, 0xbe, 0xbf, 0xc0, 0xc1, 0xc2, 0xc3,
        0xc9, 0xca, 0xcb, 0xda, 0xdb, 0xee, 0x359, 0x35a, 0x3a7,
    ];
    for opcode in opcodes {
        let decoded =
            RemoteMovement::decode(opcode, &[0; 31])?.ok_or("ordinary opcode was not decoded")?;
        assert_eq!(decoded.kind as u16, opcode);
    }
    for opcode in [0xdd, 0x2ae, 0xc7, 0x2d1, 0x38d, 0xffff] {
        assert!(RemoteMovement::decode(opcode, &[])?.is_none());
    }
    Ok(())
}
