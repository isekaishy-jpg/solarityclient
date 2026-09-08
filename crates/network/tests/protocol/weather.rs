use super::WorldServerPacket;
use crate::WorldWeatherUpdate;

#[test]
fn weather_preserves_server_grade_and_nonzero_instant_flag() {
    for grade in [-1_f32, 0., 0.25, 1., 2.] {
        for instant in [0, 1, 255] {
            let mut body = 42_u32.to_le_bytes().to_vec();
            body.extend_from_slice(&grade.to_le_bytes());
            body.push(instant);
            let packet = WorldServerPacket::new(0x2f4, body.clone());
            assert_eq!(packet.name(), Some("SMSG_WEATHER"));
            assert_eq!(
                packet.weather(),
                Ok(Some(WorldWeatherUpdate {
                    weather_id: 42,
                    grade,
                    instant: instant != 0
                }))
            );
            assert_eq!(WorldServerPacket::new(0x2f3, body).weather(), Ok(None));
        }
    }
    for length in [0, 8, 10] {
        assert!(
            WorldServerPacket::new(0x2f4, vec![0; length])
                .weather()
                .is_err()
        );
    }
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut body = vec![0; 4];
        body.extend_from_slice(&value.to_le_bytes());
        body.push(0);
        assert!(WorldServerPacket::new(0x2f4, body).weather().is_err());
    }
}
