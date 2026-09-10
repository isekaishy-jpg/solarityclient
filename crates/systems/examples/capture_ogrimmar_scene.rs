//! Captures installed Orgrimmar visibility at the world benchmark's settled pose.
//!
//! The paired native oracle reads the copied local assets and exact input floats.
//! No installed archive data is checked into the repository.

use std::{error::Error, fmt::Write, io, path::PathBuf, sync::Arc};

use glam::{Mat4, Vec3};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedWorldModel, Locale,
};
use solarity_systems::{
    MovementCollisionBounds, PlacedWorldModelCollision, WorldModelBatchVisibilityQuery,
    WorldModelCameraSceneQuery, WorldSceneCameraFrame,
};

/// Retains the full installed portal graph and each callback's local draw region.
fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: capture_ogrimmar_scene <Data directory> <output directory>",
        )
        .into());
    }
    let output = PathBuf::from(&args[1]);
    std::fs::create_dir_all(&output)?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(PathBuf::from(&args[0]))?,
        Locale::EnUs,
    )?)?;
    let path = AssetPath::new("World/Wmo/Kalimdor/Ogrimmar/Ogrimmar.wmo")?;
    let model = Arc::new(DecodedWorldModel::load(&mut store, &path)?);
    // Preserve archive selection through AssetStore, including any local patches.
    std::fs::write(output.join("ogrimmar.wmo"), store.read(&path)?.bytes())?;
    for group in model.groups() {
        std::fs::write(
            output.join(format!("ogrimmar_{:03}.wmo", group.index())),
            store.read(group.path())?.bytes(),
        )?;
    }
    // Captured MODF 165042 and the settled 1280x720 benchmark camera, in Z-up.
    let transform = Mat4::from_cols_array(&[
        -0.82903755,
        -0.55919296,
        0.,
        0.,
        0.55919296,
        -0.82903755,
        0.,
        0.,
        0.,
        0.,
        1.,
        0.,
        1500.502,
        -4418.1934,
        30.661757,
        1.,
    ]);
    let root = PlacedWorldModelCollision::prepare_transform(Arc::clone(&model), transform)?;
    let eye = Vec3::new(1075.3798, -4500., 156.24147);
    let target = Vec3::new(1076.3646, -4500., 156.06783);
    let forward = Vec3::new(0.9848077, 0., -0.17364818);
    let up = Vec3::new(0.17364818, 0., 0.9848077);
    let projection = [0.9424778, 1280. / 720., 0.2, 777.];
    let frame = WorldSceneCameraFrame::perspective(
        eye,
        target,
        forward,
        up,
        projection[0],
        projection[1],
        [projection[2], projection[3]],
    )?;
    let mut capture = String::from("inputs");
    append_floats(
        &mut capture,
        eye.to_array()
            .into_iter()
            .chain(target.to_array())
            .chain(forward.to_array())
            .chain(up.to_array())
            .chain(projection)
            .chain(transform.to_cols_array())
            .chain(root.inverse_transform().to_cols_array()),
    )?;
    capture.push('\n');
    let mut query = WorldModelCameraSceneQuery::default();
    let mut batches = WorldModelBatchVisibilityQuery::default();
    for (entry, info) in model.group_info().iter().enumerate() {
        if info.flags() & 0x10008 == 0 {
            continue;
        }
        query.query_outdoor_group(&root, frame, entry, [0., 0., 1., 1.])?;
        for visit in query.visits() {
            let clip = visit.frustum.transformed(root.inverse_transform())?;
            write!(capture, "visit {} {}", entry, visit.group)?;
            append_floats(
                &mut capture,
                clip.corners()
                    .iter()
                    .flat_map(|v| v.to_array())
                    .chain(clip.clip_planes().iter().flatten().copied()),
            )?;
            let group = &model.groups()[visit.group];
            // 7ABF50/7AC6A0 dispatch uses MOCV presence and root flag two.
            let count = if model.flags() & 2 != 0 || group.flags() & 4 != 0 {
                group.batches().len()
            } else {
                usize::from(group.batch_counts()[2])
            };
            let bounds = group.batches()[..count]
                .iter()
                .map(|batch| {
                    let [minimum, maximum] =
                        batch.bounds().map(|v| Vec3::from_array(v.map(f32::from)));
                    MovementCollisionBounds::new(minimum, maximum)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let selected = batches.query(&bounds, &[clip]);
            write!(capture, " batches {}", selected.len())?;
            for index in selected {
                write!(capture, " {index}")?;
            }
            capture.push('\n');
        }
    }
    std::fs::write(output.join("rust-scene.txt"), capture)?;
    Ok(())
}

/// Hexadecimal float stores prevent decimal round trips from changing inputs.
fn append_floats(output: &mut String, values: impl IntoIterator<Item = f32>) -> std::fmt::Result {
    for value in values {
        write!(output, " {:08x}", value.to_bits())?;
    }
    Ok(())
}
