//! Coarse rejection must preserve the original chunk visibility oracle.

use super::*;
use crate::{TerrainTileMeshPlan, WorldCamera, WorldCameraError, WorldScreenWindow};
use solarity_asset::{TerrainChunkIndex, TerrainTileIndex};
use std::error::Error;

fn chunk(index: usize, bounds: [[f32; 3]; 2]) -> Result<TerrainChunkDrawPlan, Box<dyn Error>> {
    Ok(TerrainChunkDrawPlan::new(
        TerrainChunkIndex::new((index % 16) as u8, (index / 16) as u8).ok_or("chunk index")?,
        0,
        0,
        Vec::new(),
        false,
        bounds,
    ))
}

fn tile(chunks: Vec<TerrainChunkDrawPlan>) -> Result<TerrainTileMeshPlan, Box<dyn Error>> {
    Ok(TerrainTileMeshPlan::new(
        TerrainTileIndex::new(32, 32).ok_or("tile index")?,
        Vec::new(),
        Vec::new(),
        chunks,
        Vec::new(),
        None,
        vec![0; super::super::TERRAIN_MATERIAL_ATLAS_BYTE_COUNT]
            .into_boxed_slice()
            .try_into()
            .map_err(|_| "atlas extent")?,
    ))
}

#[test]
fn tile_culling_preserves_chunk_selection_across_camera_and_world_extents()
-> Result<(), Box<dyn Error>> {
    let windows = [
        WorldScreenWindow::FULL,
        WorldScreenWindow::new(-0.7, -0.2, 0.3, 0.9),
        WorldScreenWindow::new(0.4, -0.8, 0.401, 0.8),
    ];
    let mut rejected = 0;
    let mut accepted = 0;
    for origin in [
        Vec3::ZERO,
        Vec3::new(15000., -14000., 2048.),
        Vec3::splat(33_554_432.),
    ] {
        let tiles = (-1..=1)
            .flat_map(|y| (-1..=1).map(move |x| (x, y)))
            .map(|(x, y)| {
                let base = origin + Vec3::new(x as f32 * 800., y as f32 * 800., -30.);
                tile(
                    (0..256)
                        .map(|index| {
                            let minimum = base
                                + Vec3::new(
                                    (index % 16) as f32 * (1600. / 48.),
                                    (index / 16) as f32 * (1600. / 48.),
                                    (index % 17) as f32 * 2.,
                                );
                            let maximum = minimum
                                + Vec3::new(1600. / 48., 1600. / 48., (index % 7) as f32 * 11.);
                            chunk(index, [minimum.to_array(), maximum.to_array()])
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        for sample in 0..24 {
            let angle = sample as f32 * std::f32::consts::TAU / 24.;
            let direction =
                Vec3::new(angle.cos(), angle.sin(), (sample % 5) as f32 * 0.2 - 0.4).normalize();
            let eye = origin + Vec3::new(128., 256., 50.) - direction * 600.;
            for far in [64., 533., 2000.] {
                for parallel in [false, true] {
                    let camera = if parallel {
                        WorldCamera::orthographic(
                            eye,
                            eye + direction,
                            Vec3::Z,
                            [-70., 400.],
                            [-200., 300.],
                            -10.,
                            far,
                        )
                    } else {
                        WorldCamera::stock(eye, eye + direction, Vec3::Z, far)
                    }
                    .with_view_direction(direction)
                    .frame(16. / 9.)?;
                    for window in windows {
                        let frustum = WorldFrustum::new(camera, window)?;
                        for tile in &tiles {
                            let coarse = tile.may_have_visible_chunks(frustum);
                            if coarse {
                                accepted += 1;
                            } else {
                                rejected += 1;
                            }
                            let expected = tile
                                .chunks()
                                .iter()
                                .enumerate()
                                .map(|(index, chunk)| {
                                    chunk
                                        .is_visible(frustum)
                                        .map(|visible| visible.then_some(index))
                                })
                                .collect::<Result<Vec<_>, _>>()?;
                            let actual = tile
                                .chunks()
                                .iter()
                                .enumerate()
                                .map(|(index, chunk)| {
                                    if coarse {
                                        chunk
                                            .is_visible(frustum)
                                            .map(|visible| visible.then_some(index))
                                    } else {
                                        Ok(None)
                                    }
                                })
                                .collect::<Result<Vec<_>, WorldCameraError>>()?;
                            assert_eq!(
                                actual, expected,
                                "origin {origin:?}, sample {sample}, far {far}, parallel {parallel}, window {window:?}"
                            );
                        }
                    }
                }
            }
        }
    }
    assert!(
        rejected > accepted,
        "the fixture must exercise substantial coarse rejection"
    );
    assert!(accepted > 0);
    Ok(())
}

#[test]
fn tile_culling_keeps_boundary_boxes_and_preserves_invalid_bound_errors()
-> Result<(), Box<dyn Error>> {
    let frame = WorldCamera::orthographic(
        Vec3::ZERO,
        Vec3::X,
        Vec3::Z,
        [-1., 1.],
        [-1., 1.],
        0.2,
        100.,
    )
    .frame(1.)?;
    let frustum = WorldFrustum::new(frame, WorldScreenWindow::FULL)?;
    for axis in 0..3 {
        for boundary in [-100_f32, -1., 0.2, 1., 100.] {
            for value in [boundary.next_down(), boundary, boundary.next_up()] {
                let mut center = Vec3::new(1., 0., 0.);
                center[axis] = value;
                for half in [0., 0.125, 16.] {
                    let minimum = center - Vec3::splat(half);
                    let maximum = center + Vec3::splat(half);
                    let chunks = [chunk(0, [minimum.to_array(), maximum.to_array()])?];
                    assert_eq!(
                        TerrainTileCulling::new(&chunks)
                            .ok_or("valid bounds")?
                            .may_have_visible_chunks(frustum),
                        chunks[0].is_visible(frustum)?,
                        "axis {axis}, value {value:?}, half {half}"
                    );
                }
            }
        }
    }
    assert!(!tile(Vec::new())?.may_have_visible_chunks(frustum));
    for value in [f32::NAN, f32::INFINITY, f32::MAX] {
        let plan = tile(vec![chunk(0, [[value; 3]; 2])?])?;
        assert!(plan.may_have_visible_chunks(frustum));
        assert_eq!(
            plan.chunks()[0].is_visible(frustum),
            Err(WorldCameraError::NonFiniteBounds)
        );
    }
    // Finite inputs with overflowing plane expressions also stay on the
    // original path. Different signs can turn a member's dot product into NaN.
    let wide = WorldCamera::stock(Vec3::ZERO, Vec3::ONE, Vec3::Z, 100.).frame(16.)?;
    let wide = WorldFrustum::new(wide, WorldScreenWindow::FULL)?;
    let extreme = tile(vec![chunk(
        0,
        [[1.0e38, -1.0e38, -1.], [1.0e38, -1.0e38, 3.]],
    )?])?;
    assert!(
        extreme.chunks()[0].is_visible(wide)?,
        "legacy overflowing dot product"
    );
    assert!(extreme.may_have_visible_chunks(wide));
    Ok(())
}
