//! Native first acceptance across registered group and moving-root submissions.

use solarity_systems::WorldSceneCameraFrame;
use std::error::Error;

use super::*;

#[test]
fn registered_model_fog_matches_original_group_submission() -> Result<(), Box<dyn Error>> {
    let input = include_str!("../../../systems/tests/fixtures/world_scene_projection_native.txt")
        .lines()
        .find(|line| !line.starts_with('#'))
        .ok_or("camera")?
        .split_whitespace()
        .take(16)
        .map(float)
        .collect::<Result<Vec<_>, _>>()?;
    let camera = WorldSceneCameraFrame::perspective(
        Vec3::from_slice(&input[..3]),
        Vec3::from_slice(&input[3..6]),
        Vec3::from_slice(&input[6..9]),
        Vec3::from_slice(&input[9..12]),
        input[12],
        input[13],
        [input[14], input[15]],
    )?;
    let mut graphics = WorldModelSceneGraphics::default();
    let mut colors = [Vec3::ZERO; 2];
    let mut count = 0;
    for line in include_str!("../fixtures/model_owner_fog_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        match fields[0] {
            "colors" => {
                for index in 0..2 {
                    let color = u32::from_str_radix(fields[index + 1], 16)?;
                    colors[index] = Vec3::new(
                        ((color >> 16) & 255) as f32,
                        ((color >> 8) & 255) as f32,
                        (color & 255) as f32,
                    ) / 255.;
                }
            }
            "scene" => graphics = WorldModelSceneGraphics::default(),
            "group" => {
                let (owner, group) = key(fields[1].parse()?);
                let clips = fields[5..]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|window| {
                        camera
                            .frustum_for_window([
                                float(window[0])?,
                                float(window[1])?,
                                float(window[2])?,
                                float(window[3])?,
                            ])
                            .map_err(Into::into)
                    })
                    .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
                assert_eq!(clips.len(), fields[4].parse::<usize>()?);
                graphics
                    .indices
                    .insert((owner, group), graphics.groups.len());
                graphics.groups.push(WorldModelSceneGroup {
                    owner,
                    group,
                    indoor_fog: fields[3] == "1",
                    frusta: Vec::new(),
                    doodads: WorldModelSceneDoodads {
                        world_frusta: clips,
                        references: Vec::new(),
                        depth: 0.,
                        allowed: fields[2] == "1",
                    },
                });
                graphics.active = graphics.groups.len();
            }
            "direct" => graphics.direct_doodad_visits.push(DirectDoodadVisit {
                group: *graphics
                    .indices
                    .get(&key(fields[1].parse()?))
                    .ok_or("direct group")?,
                clip_count: fields[2].parse()?,
                indoor_fog: fields[3] == "1",
            }),
            "case" => {
                let mask: u32 = fields[3].parse()?;
                let mut references = (0..3)
                    .filter(|&id| mask & (1 << id) != 0)
                    .map(|id| {
                        let (owner, group) = key(id);
                        match owner {
                            RuntimeWorldModelMovementOwner::Static { unique_id } => {
                                RuntimeMovementReference::WorldModel { unique_id, group }
                            }
                            RuntimeWorldModelMovementOwner::GameObject { identity } => {
                                RuntimeMovementReference::GameObjectWorldModel { identity, group }
                            }
                        }
                    })
                    .collect::<Vec<_>>();
                if fields[4] == "1" {
                    references.reverse();
                }
                let center = Vec3::new(float(fields[5])?, float(fields[6])?, float(fields[7])?);
                let actual = graphics
                    .registered_model_fog(
                        &references,
                        fields[2] == "1",
                        (center, float(fields[8])?),
                    )
                    .unwrap_or(fields[1] == "1");
                assert_eq!(actual, fields[9] == "1", "{line}");
                let native = Vec3::new(float(fields[10])?, float(fields[11])?, float(fields[12])?);
                assert!(
                    native.abs_diff_eq(colors[usize::from(actual)], 1e-7),
                    "{line}"
                );
                count += 1;
            }
            _ => return Err("unexpected native row".into()),
        }
    }
    assert_eq!(count, 1280);
    Ok(())
}

fn key(id: usize) -> (RuntimeWorldModelMovementOwner, usize) {
    (
        RuntimeWorldModelMovementOwner::Static {
            unique_id: if id == 2 { 200 } else { 100 },
        },
        id % 2,
    )
}

fn float(word: &str) -> Result<f32, std::num::ParseIntError> {
    Ok(f32::from_bits(u32::from_str_radix(word, 16)?))
}
