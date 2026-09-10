//! Native palette slots and realm-clock phase requests.

use super::*;

#[test]
fn world_model_skybox_replacement_matches_native_slot_writes()
-> Result<(), Box<dyn std::error::Error>> {
    for line in include_str!("../fixtures/world_model_sky_native.txt")
        .lines()
        .filter(|line| line.starts_with("slots "))
    {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        let words = fields[1..]
            .iter()
            .map(|word| u32::from_str_radix(word, 16))
            .collect::<Result<Vec<_>, _>>()?;
        let mut slots = std::array::from_fn(|i| SkyboxSlot {
            model: Some(10 + i),
            weight: [0.1, 0.2, 0.3][i],
            flags: 1 + i as u32,
        });
        replace_world_model(&mut slots, Some(99), f32::from_bits(words[0]));
        for (i, slot) in slots.into_iter().enumerate() {
            assert_eq!(slot.model.unwrap_or(0) as u32, words[1 + i], "{line}");
            assert_eq!(slot.weight.to_bits(), words[4 + i], "{line}");
            assert_eq!(slot.flags, words[7 + i], "{line}");
        }
    }
    Ok(())
}

#[test]
fn native_skybox_requests_and_slots() -> Result<(), Box<dyn std::error::Error>> {
    let mut phase = SkyboxPhase::default();
    for (line_index, line) in include_str!("../fixtures/world_skybox_native.txt")
        .lines()
        .enumerate()
    {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        let hex = |i: usize| u32::from_str_radix(fields[i], 16);
        match fields.first().copied() {
            Some("reset") => phase = SkyboxPhase::default(),
            Some("phase") => {
                phase.flags = fields[2].parse()?;
                let request =
                    phase.update(fields[3] == "1", fields[1].parse()?, fields[4].parse()?);
                assert_eq!(
                    phase.duration,
                    fields[5].parse::<u32>()?,
                    "line {line_index}"
                );
                assert_eq!(phase.last_minute as u32, hex(6)?, "line {line_index}");
                if fields[7] == "-" {
                    assert!(request.is_none(), "line {line_index}");
                } else {
                    let (offset, speed) = request.ok_or("missing native skybox seek")?;
                    assert_eq!(
                        [u32::MAX, 0, u32::MAX, offset as u32, speed.to_bits(), 1, 1],
                        [
                            hex(7)?,
                            hex(8)?,
                            hex(9)?,
                            hex(10)?,
                            hex(11)?,
                            hex(12)?,
                            hex(13)?
                        ],
                        "line {line_index}"
                    );
                }
            }
            Some("slots") => {
                let inputs = [
                    (fields[1].parse()?, f32::from_bits(hex(4)?)),
                    (fields[2].parse()?, f32::from_bits(hex(5)?)),
                    (fields[3].parse()?, f32::from_bits(hex(6)?)),
                ];
                let slots = select_slots::<std::convert::Infallible>(inputs, |id| {
                    Ok(match id {
                        1 => Some((Some(100), 0)),
                        2 => Some((Some(101), 2)),
                        3 => Some((Some(102), 1)),
                        4 => Some((None, 0)),
                        _ => None,
                    })
                })?;
                for (i, slot) in slots.into_iter().enumerate() {
                    assert_eq!(
                        slot.model.unwrap_or(0) as u32,
                        hex(7 + i)?,
                        "line {line_index}"
                    );
                    assert_eq!(slot.weight.to_bits(), hex(10 + i)?, "line {line_index}");
                    assert_eq!(slot.flags, hex(13 + i)?, "line {line_index}");
                }
            }
            _ => {}
        }
    }
    Ok(())
}
