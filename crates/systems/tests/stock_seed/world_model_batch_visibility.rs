//! Local scene-frustum stores and ordered MOBA selection from original code.

use glam::{Mat4, Vec3};
use solarity_systems::{
    MovementCollisionBounds, WorldModelBatchVisibilityQuery, WorldModelVisibilityError,
    WorldModelVisibilityVisit, WorldSceneCameraFrame,
};
use std::error::Error;

/// Decodes the native fixture's stored single-precision bit patterns.
fn float(word: &str) -> Result<f32, Box<dyn Error>> {
    Ok(f32::from_bits(u32::from_str_radix(word, 16)?))
}

/// Reconstructs the independently captured camera inputs, not output corners.
fn cameras() -> Result<Vec<WorldSceneCameraFrame>, Box<dyn Error>> {
    include_str!("../fixtures/world_scene_projection_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let values = line
                .split_ascii_whitespace()
                .map(float)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(WorldSceneCameraFrame::perspective(
                Vec3::from_slice(&values[..3]),
                Vec3::from_slice(&values[3..6]),
                Vec3::from_slice(&values[6..9]),
                Vec3::from_slice(&values[9..12]),
                values[12],
                values[13],
                [values[14], values[15]],
            )?)
        })
        .collect()
}

/// Invalid transforms cannot publish a malformed local clipping region.
#[test]
fn local_batch_frusta_reject_nonfinite_and_collapsed_transforms() -> Result<(), Box<dyn Error>> {
    let camera = cameras()?[0];
    let frustum = camera.frustum_for_window([0., 0., 1., 1.])?;
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(matches!(
            frustum.transformed(Mat4::from_translation(Vec3::splat(value))),
            Err(WorldModelVisibilityError::NonFiniteCoordinates)
        ));
    }
    assert!(matches!(
        frustum.transformed(Mat4::from_scale(Vec3::ZERO)),
        Err(WorldModelVisibilityError::DegenerateFrustum)
    ));
    Ok(())
}

/// Executes the complete native crop and local transform, including six planes.
#[test]
fn local_batch_frusta_match_original_corner_transform_and_plane_rebuild()
-> Result<(), Box<dyn Error>> {
    let cameras = cameras()?;
    let mut count = 0;
    for line in include_str!("../fixtures/world_model_local_frusta_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let mut fields = line.split_ascii_whitespace();
        let camera = cameras[fields.next().ok_or("missing camera")?.parse::<usize>()?];
        let values = fields.map(float).collect::<Result<Vec<_>, _>>()?;
        let local = camera
            .frustum_for_window(values[16..20].try_into()?)?
            .transformed(Mat4::from_cols_slice(&values[..16]))?;
        for (channel, (actual, expected)) in local
            .corners()
            .iter()
            .flat_map(|v| v.to_array())
            .chain(local.clip_planes().iter().flatten().copied())
            .zip(&values[20..])
            .enumerate()
        {
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "local frustum {count}/{channel}: {actual} != {expected}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 432);
    Ok(())
}

/// Reads one required field without silently truncating native records.
fn field<'a>(fields: &mut impl Iterator<Item = &'a str>) -> Result<&'a str, Box<dyn Error>> {
    fields
        .next()
        .ok_or_else(|| "missing native batch field".into())
}

/// Original recursive push/pop/crop code retains inherited initial clips.
#[test]
fn portal_callback_frusta_match_original_camera_and_outdoor_clip_stacks()
-> Result<(), Box<dyn Error>> {
    let cameras = cameras()?;
    let mut count = 0;
    for line in include_str!("../fixtures/world_model_scene_clip_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let mut fields = line.split_ascii_whitespace();
        let camera = cameras[field(&mut fields)?.parse::<usize>()?];
        let outdoor = field(&mut fields)? == "1";
        let mut window = [0.; 4];
        for value in &mut window {
            *value = float(field(&mut fields)?)?;
        }
        let group = field(&mut fields)?.parse::<usize>()?;
        let indoor_fog = field(&mut fields)? == "1";
        let depth = field(&mut fields)?.parse::<u32>()?;
        let mut screen_window = [0.; 4];
        for value in &mut screen_window {
            *value = float(field(&mut fields)?)?;
        }
        let inherited = if outdoor {
            camera.frustum_for_window(window)?
        } else {
            camera.frustum()
        };
        let frustum = WorldModelVisibilityVisit {
            group,
            indoor_fog,
            depth,
            screen_window,
        }
        .frustum(camera, inherited)?;
        let expected = fields.map(float).collect::<Result<Vec<_>, _>>()?;
        assert_eq!(expected.len(), 48);
        for (channel, (actual, expected)) in frustum
            .corners()
            .iter()
            .flat_map(|v| v.to_array())
            .chain(frustum.clip_planes().iter().flatten().copied())
            .zip(expected)
            .enumerate()
        {
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "callback {count}/{channel}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 360);
    Ok(())
}

/// Native selection includes disjoint windows, duplicate revisits, initial
/// high-nibble garbage, signed bounds, and groups reused in the next frame.
#[test]
fn batch_visibility_matches_original_multi_frustum_order_and_duplicate_markers()
-> Result<(), Box<dyn Error>> {
    let cameras = cameras()?;
    let mut query = WorldModelBatchVisibilityQuery::default();
    let mut count = 0;
    let mut reordered = 0;
    for line in include_str!("../fixtures/world_model_batch_visibility_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let mut fields = line.split_ascii_whitespace();
        let camera = cameras[field(&mut fields)?.parse::<usize>()?];
        let matrix = (0..16)
            .map(|_| float(field(&mut fields)?))
            .collect::<Result<Vec<_>, _>>()?;
        let matrix = Mat4::from_cols_slice(&matrix);
        let batch_count = field(&mut fields)?.parse::<usize>()?;
        let mut bounds = Vec::new();
        let mut flags = Vec::new();
        for _ in 0..batch_count {
            let mut values = [0.; 6];
            for value in &mut values {
                *value = f32::from(field(&mut fields)?.parse::<i16>()?);
            }
            bounds.push(MovementCollisionBounds::new(
                Vec3::from_slice(&values[..3]),
                Vec3::from_slice(&values[3..]),
            )?);
            flags.push(field(&mut fields)?.parse::<u8>()?);
        }
        let window_count = field(&mut fields)?.parse::<usize>()?;
        let mut frusta = Vec::new();
        let mut expected = Vec::new();
        for window_index in 0..window_count {
            let mut window = [0.; 4];
            for value in &mut window {
                *value = float(field(&mut fields)?)?;
            }
            frusta.push(camera.frustum_for_window(window)?.transformed(matrix)?);
            let selected_count = field(&mut fields)?.parse::<usize>()?;
            for _ in 0..selected_count {
                expected.push(field(&mut fields)?.parse::<usize>()?);
            }
            assert_eq!(
                query.query(&bounds, &frusta),
                expected,
                "case {count} window {window_index}"
            );
        }
        reordered += usize::from(expected.windows(2).any(|pair| pair[0] > pair[1]));
        for (index, flag) in flags.into_iter().enumerate() {
            let native_final = field(&mut fields)?.parse::<u8>()?;
            let selected_flag = if expected.contains(&index) { 0xf0 } else { 0 };
            assert_eq!(
                native_final,
                flag & 15 | selected_flag,
                "case {count} batch {index}"
            );
        }
        assert!(fields.next().is_none());
        assert!(query.query(&bounds, &[]).is_empty());
        assert_eq!(
            query.query(&bounds, &frusta),
            expected,
            "next frame {count}"
        );
        assert!(query.query(&[], &frusta).is_empty());
        count += 1;
    }
    assert_eq!(count, 72);
    assert!(
        reordered > 0,
        "fixtures must distinguish portal order from global batch order"
    );
    Ok(())
}
