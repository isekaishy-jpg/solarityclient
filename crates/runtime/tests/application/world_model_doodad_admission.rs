//! Original group order, clipping, class gates and repeated-frame fog selection.

use super::*;
use glam::Mat4;
use solarity_systems::WorldSceneCameraFrame;
use std::error::Error;

#[test]
fn attached_model_admission_matches_native_group_sequence() -> Result<(), Box<dyn Error>> {
    let camera_input =
        include_str!("../../../systems/tests/fixtures/world_scene_projection_native.txt")
            .lines()
            .find(|line| !line.starts_with('#'))
            .ok_or("camera")?
            .split_whitespace()
            .take(16)
            .map(float)
            .collect::<Result<Vec<_>, _>>()?;
    let camera = WorldSceneCameraFrame::perspective(
        Vec3::from_slice(&camera_input[..3]),
        Vec3::from_slice(&camera_input[3..6]),
        Vec3::from_slice(&camera_input[6..9]),
        Vec3::from_slice(&camera_input[9..12]),
        camera_input[12],
        camera_input[13],
        [camera_input[14], camera_input[15]],
    )?;
    let mut scene = M2DoodadScene::default();
    let mut frame = 0;
    let mut submitted = Vec::new();
    let mut colors = [Vec3::ZERO; 2];
    let mut queries = 0;
    let mut visits = 0;
    for line in include_str!("../fixtures/world_model_doodad_admission_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let fields: Vec<_> = line.split_whitespace().collect();
        match fields[0] {
            "colors" => {
                for (index, word) in fields[1..].iter().enumerate() {
                    let color = u32::from_str_radix(word, 16)?;
                    colors[index] = Vec3::new(
                        ((color >> 16) & 255) as f32,
                        ((color >> 8) & 255) as f32,
                        (color & 255) as f32,
                    ) / 255.;
                }
            }
            "model" => {
                let values = fields[1..]
                    .iter()
                    .map(|word| float(word))
                    .collect::<Result<Vec<_>, _>>()?;
                let center = Vec3::from_slice(&values[1..4]);
                let half_extent = Vec3::new(values[0] * 0.5, 0.1, 0.1);
                scene.spheres.push(Some((
                    center,
                    values[4],
                    SceneryDistance::new(-half_extent, half_extent, Mat4::from_translation(center)),
                )));
                scene.fog_banks.push(None);
            }
            "visit" => {
                let next_frame: usize = fields[1].parse()?;
                if next_frame != frame {
                    scene.fog_banks.fill(None);
                    frame = next_frame;
                }
                let depth = float(fields[2])?;
                let detail = float(fields[3])?;
                let bank = fields[4] == "1";
                let count: usize = fields[5].parse()?;
                let reference_ids = fields[6..6 + count]
                    .iter()
                    .map(|word| word.parse::<usize>())
                    .collect::<Result<Vec<_>, _>>()?;
                let clip_count: usize = fields[6 + count].parse()?;
                let windows = fields[7 + count..]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|values| {
                        camera
                            .frustum_for_window([
                                float(values[0])?,
                                float(values[1])?,
                                float(values[2])?,
                                float(values[3])?,
                            ])
                            .map_err(Into::into)
                    })
                    .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
                assert_eq!(windows.len(), clip_count);
                submitted.clear();
                for index in reference_ids {
                    let before = scene.fog_bank(index);
                    scene.admit(index, &windows, depth, detail, bank);
                    if before.is_none() && scene.fog_bank(index).is_some() {
                        submitted.push(index);
                    }
                }
                visits += 1;
            }
            "result" => {
                for (index, values) in fields[1..].as_chunks::<4>().0.iter().enumerate() {
                    let expected = match values[0] {
                        "0" => Some(false),
                        "1" => Some(true),
                        "-1" => None,
                        _ => return Err("fog flag".into()),
                    };
                    assert_eq!(
                        scene.fog_bank(index),
                        expected,
                        "visit {visits}, model {index}"
                    );
                    if let Some(bank) = expected {
                        let color =
                            Vec3::new(float(values[1])?, float(values[2])?, float(values[3])?);
                        assert!(
                            color.abs_diff_eq(colors[usize::from(bank)], 1.0e-7),
                            "native model query {queries}"
                        );
                    }
                    queries += 1;
                }
            }
            "submitted" => {
                let count: usize = fields[1].parse()?;
                let expected = fields[2..]
                    .iter()
                    .map(|word| word.parse::<usize>())
                    .collect::<Result<Vec<_>, _>>()?;
                assert_eq!(count, expected.len());
                assert_eq!(submitted, expected, "visit {visits}");
            }
            _ => return Err("unknown fixture row".into()),
        }
    }
    assert_eq!((visits, queries), (32, 160));
    Ok(())
}

fn float(word: &str) -> Result<f32, Box<dyn Error>> {
    Ok(f32::from_bits(u32::from_str_radix(word, 16)?))
}

#[test]
fn exterior_doodad_buckets_and_fog_match_native_submission() -> Result<(), Box<dyn Error>> {
    let camera = WorldSceneCameraFrame::perspective(
        Vec3::ZERO,
        Vec3::X,
        Vec3::X,
        Vec3::Z,
        f32::from_bits(0x3f71_463a),
        1.,
        [0.2, 100.],
    )?;
    let clip = camera.frustum_for_window([0., 0., 1., 1.])?;
    let depth = camera.depth_frame()?;
    let mut scene = M2DoodadScene::default();
    scene.outdoor_bins.resize_with(64, Vec::new);
    scene.spheres.push(None);
    scene.queued.push(false);
    scene.fog_banks.push(None);
    scene.retained_fog.push(false);
    let mut count = 0;
    for line in include_str!("../fixtures/world_model_doodad_outdoor_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let fields: Vec<_> = line.split_whitespace().collect();
        let center = Vec3::new(float(fields[0])?, float(fields[1])?, float(fields[2])?);
        let radius = float(fields[3])?;
        let extent = float(fields[4])?;
        let detail = float(fields[5])?;
        let minimum: u8 = fields[6].parse()?;
        let bank = fields[7] == "1";
        let expected_bin: i32 = fields[8].parse()?;
        let bin = depth.doodad_depth_bin(center, radius, minimum)?;
        assert_eq!(
            bin.map_or(-1, i32::from),
            expected_bin,
            "queue case {count}"
        );
        let half_extent = Vec3::new(extent * 0.5, 0.1, 0.1);
        scene.spheres[0] = Some((
            center,
            radius,
            SceneryDistance::new(-half_extent, half_extent, Mat4::from_translation(center)),
        ));
        scene.fog_banks[0] = None;
        scene.retained_fog[0] = bank;
        if let Some(bin) = bin {
            scene.outdoor_bins[usize::from(bin)].push(0);
            scene.queued[0] = true;
            scene.submit_outdoor_bin(usize::from(bin), clip, detail);
            scene.outdoor_bins[usize::from(bin)].clear();
        }
        assert_eq!(
            scene.fog_bank(0),
            (fields[9] == "1").then_some(bank),
            "submission case {count}"
        );
        assert_eq!(
            bank,
            fields[10] == "1",
            "outdoor submission retains native bank"
        );
        let expected_color = if bank {
            Vec3::new(192., 128., 32.)
        } else {
            Vec3::new(32., 64., 96.)
        } / 255.;
        let native_color = Vec3::new(float(fields[11])?, float(fields[12])?, float(fields[13])?);
        assert!(expected_color.abs_diff_eq(native_color, 1.0e-7));
        count += 1;
    }
    assert_eq!(count, 288);
    Ok(())
}
