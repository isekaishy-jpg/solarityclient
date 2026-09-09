//! Original ordinary-player follow requests and timed interpolation.

use super::{FollowAngle, FollowSettings, request};

#[test]
fn follow_histories_match_original_policy_and_interpolation()
-> Result<(), Box<dyn std::error::Error>> {
    for (index, line) in include_str!("../fixtures/camera-follow-native.txt")
        .lines()
        .skip(1)
        .enumerate()
    {
        let words = line
            .split_whitespace()
            .filter(|word| *word != "|")
            .map(|word| u32::from_str_radix(word, 16))
            .collect::<Result<Vec<_>, _>>()?;
        let mut settings = FollowSettings {
            style: words[0] as usize,
            ..Default::default()
        };
        if words[6] != 0 {
            settings.conditions = [[[0.125, 2.5]; 7]; 5];
        }
        let mut angles = [
            FollowAngle::new(f32::from_bits(words[3])),
            FollowAngle::new(f32::from_bits(words[2])),
        ];
        for step in words[7..].as_chunks::<17>().0 {
            if step[0] == 0 {
                request(
                    &mut angles,
                    words[4],
                    words[1],
                    false,
                    step[1],
                    &settings,
                    false,
                );
            } else {
                angles[0].sample(step[1]);
                if words[4] & 1 == 0 {
                    angles[1].sample(step[1]);
                }
            }
            let mut actual = vec![
                words[4]
                    | if angles[0].active { 0x0200_0000 } else { 0 }
                    | if angles[1].active { 0x0100_0000 } else { 0 },
                angles[1].current.to_bits(),
                angles[0].current.to_bits(),
            ];
            for angle in angles {
                actual.extend([
                    angle.start,
                    angle.duration.to_bits(),
                    angle.goal.to_bits(),
                    angle.anchor.to_bits(),
                    angle.factor.to_bits(),
                    angle.delay.to_bits(),
                ]);
            }
            assert_eq!(
                actual,
                step[2..],
                "history {index}, kind {} time {}",
                step[0],
                step[1]
            );
        }
    }
    Ok(())
}
