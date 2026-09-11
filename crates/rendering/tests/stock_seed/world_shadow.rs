use glam::Vec3;
use solarity_rendering::{WorldShadowProjection, WorldShadowProjectionError, WorldShadowQuality};

/// 7BAFD0 keeps its asymmetric crop multiplication; 983A60 excludes full boxes.
#[test]
fn environment_volume_admission_and_containment_match_original()
-> Result<(), Box<dyn std::error::Error>> {
    let mut cases = 0;
    for line in include_str!("../fixtures/world-shadow-volume-native.txt")
        .lines()
        .filter(|line| line.starts_with("volume "))
    {
        let values = line
            .split_whitespace()
            .skip(1)
            .map(str::parse::<f32>)
            .collect::<Result<Vec<_>, _>>()?;
        let vector = |index| Vec3::from_slice(&values[index..index + 3]);
        let map = solarity_rendering::WorldEnvironmentShadowMap::ALL[values[0] as usize];
        let camera_minimum = vector(8);
        let camera_maximum = vector(11);
        let eye = Vec3::new(0., 0., camera_maximum.z + 1.);
        let camera = solarity_rendering::WorldCamera::orthographic(
            eye,
            eye - Vec3::Z,
            Vec3::Y,
            [camera_minimum.x, camera_maximum.x],
            [camera_minimum.y, camera_maximum.y],
            1.,
            camera_maximum.z - camera_minimum.z + 1.,
        )
        .frame(1.)?;
        for origin in [Vec3::ZERO, vector(2) + Vec3::new(7., -4., 3.)] {
            let mut projection = WorldShadowProjection::environment(
                WorldShadowQuality::EnvironmentHigh,
                map,
                vector(2),
                origin,
                vector(5),
            )?
            .with_caster_region(values[14..18].try_into()?)?;
            if values[1] != 0. {
                projection = projection.with_camera_culling(camera);
            }
            assert_eq!(
                projection.admits_bounds(vector(18), vector(21)),
                values[24] != 0.,
                "admission: {line}, origin={origin}"
            );
            assert_eq!(
                projection.contains_bounds(vector(18), vector(21)),
                values[25] != 0.,
                "containment: {line}, origin={origin}"
            );
        }
        cases += 1;
    }
    assert_eq!(cases, 18_816);
    Ok(())
}

#[test]
fn environment_projections_match_original_receiver_rows() -> Result<(), Box<dyn std::error::Error>>
{
    let mut cases = 0;
    for line in include_str!("../fixtures/world-environment-shadow-projection-native.txt")
        .lines()
        .filter(|line| line.starts_with("environment "))
    {
        let values = line
            .split_whitespace()
            .skip(1)
            .map(str::parse::<f32>)
            .collect::<Result<Vec<_>, _>>()?;
        let vector = |index| Vec3::from_slice(&values[index..index + 3]);
        let quality = if values[0] == 1024. {
            WorldShadowQuality::EnvironmentLow
        } else {
            WorldShadowQuality::EnvironmentHigh
        };
        let map = solarity_rendering::WorldEnvironmentShadowMap::ALL[values[1] as usize];
        let projection =
            WorldShadowProjection::environment(quality, map, vector(2), vector(5), vector(8))?;
        assert_eq!(projection.receiver_center(), vector(2));
        assert!(
            (projection.light_direction() - vector(11))
                .abs()
                .max_element()
                < 0.000001
        );
        for (index, (actual, expected)) in projection
            .caster_view()
            .to_cols_array()
            .into_iter()
            .zip(&values[14..30])
            .enumerate()
        {
            assert!(
                (actual - expected).abs() < 0.0015,
                "case {cases} view {index}: {actual} versus {expected}"
            );
        }
        for (index, (actual, expected)) in projection
            .receiver_rows()
            .into_iter()
            .flat_map(|row| row.to_array())
            .zip(&values[30..])
            .enumerate()
        {
            assert!(
                (actual - expected).abs() < 0.00015,
                "case {cases} receiver {index}: {actual} versus {expected}"
            );
        }
        cases += 1;
    }
    assert_eq!(cases, 72);
    Ok(())
}

/// Original shadowCull=1 transforms eight frustum corners before cropping admission.
#[test]
fn primary_unit_admission_matches_original_camera_crop() -> Result<(), Box<dyn std::error::Error>> {
    let mut cases = 0;
    for line in include_str!("../fixtures/world-shadow-caster-cropped-native.txt")
        .lines()
        .filter(|line| line.starts_with("unit "))
    {
        let values = line
            .split_whitespace()
            .skip(1)
            .map(str::parse::<f32>)
            .collect::<Result<Vec<_>, _>>()?;
        let vec = |start| Vec3::from_slice(&values[start..start + 3]);
        let minimum = vec(6);
        let maximum = vec(9);
        let eye = Vec3::new(0., 0., maximum.z + 1.);
        let camera = solarity_rendering::WorldCamera::orthographic(
            eye,
            eye - Vec3::Z,
            Vec3::Y,
            [minimum.x, maximum.x],
            [minimum.y, maximum.y],
            1.,
            maximum.z - minimum.z + 1.,
        )
        .frame(1.)?;
        for origin in [Vec3::ZERO, vec(0) + Vec3::new(7., -4., 3.)] {
            let projection = WorldShadowProjection::primary(
                WorldShadowQuality::UnitsHigh,
                vec(0),
                origin,
                vec(3),
            )?
            .with_camera_culling(camera);
            assert_eq!(
                projection.admits_unit(vec(12), vec(15), values[18]),
                values[19] != 0.,
                "{line}, origin={origin}"
            );
        }
        cases += 1;
    }
    assert_eq!(cases, 3600);
    Ok(())
}

/// Original 875F80 and 7BB9D0 execute volume setup and admission in the fixture.
#[test]
fn primary_unit_admission_matches_original_world_volume() -> Result<(), Box<dyn std::error::Error>>
{
    let mut cases = 0;
    for line in include_str!("../fixtures/world-shadow-caster-native.txt")
        .lines()
        .filter(|line| line.starts_with("unit "))
    {
        let values = line
            .split_whitespace()
            .skip(1)
            .map(str::parse::<f32>)
            .collect::<Result<Vec<_>, _>>()?;
        let vec = |start| Vec3::from_slice(&values[start..start + 3]);
        for origin in [Vec3::ZERO, vec(0) + Vec3::new(7., -4., 3.)] {
            let projection = WorldShadowProjection::primary(
                WorldShadowQuality::UnitsHigh,
                vec(0),
                origin,
                vec(3),
            )?;
            assert_eq!(
                projection.admits_unit(vec(6), vec(9), values[12]),
                values[13] != 0.,
                "{line}, origin={origin}"
            );
        }
        cases += 1;
    }
    assert_eq!(cases, 1800);
    Ok(())
}

#[test]
fn m2_caster_material_matches_original_834660_queues() -> Result<(), Box<dyn std::error::Error>> {
    use solarity_asset::{M2Batch, M2BlendMode};
    use solarity_rendering::M2ShadowMaterial;
    let modes = [
        M2BlendMode::Opaque,
        M2BlendMode::AlphaKey,
        M2BlendMode::Alpha,
        M2BlendMode::NoAlphaAdd,
        M2BlendMode::Add,
        M2BlendMode::Mod,
        M2BlendMode::Mod2x,
    ];
    let mut count = 0;
    for line in include_str!("../fixtures/m2-shadow-material-native.txt")
        .lines()
        .filter(|line| line.starts_with("m2_shadow "))
    {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        let batch = M2Batch {
            flags: fields[1].parse()?,
            priority_plane: 0,
            shader_id: fields[2].parse()?,
            skin_section_index: 0,
            geoset_index: 0,
            color_index: 0,
            material_index: 0,
            material_layer: fields[3].parse()?,
            texture_count: 1,
            texture_combo_index: 0,
            texture_coordinate_combo_index: 0,
            texture_weight_combo_index: 0,
            texture_transform_combo_index: 0,
        };
        let opacity =
            fields[6].parse::<f32>()? * fields[7].parse::<f32>()? * fields[8].parse::<f32>()?;
        let flags = fields[4].parse()?;
        let mode = modes[fields[5].parse::<usize>()?];
        let selected = (fields[9] == "1")
            .then(|| M2ShadowMaterial::select(batch, flags, mode, opacity))
            .flatten();
        let queue = match selected {
            None => 0,
            Some(M2ShadowMaterial::Opaque) => 1,
            Some(M2ShadowMaterial::AlphaTest) => 2,
        };
        assert_eq!(queue, fields[10].parse::<u8>()?, "{line}");
        count += 1;
    }
    assert_eq!(count, 1344);
    assert_eq!(M2ShadowMaterial::AlphaTest.alpha_reference(), 128.0 / 255.0);
    Ok(())
}

#[test]
fn primary_shadow_matches_original_light_camera_and_receiver_transforms()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = include_str!("../fixtures/world-shadow-projection-native.txt");
    let mut cases = 0;
    for line in fixture.lines().filter(|line| !line.starts_with('#')) {
        let values = line
            .split_whitespace()
            .skip(1)
            .map(str::parse::<f32>)
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(values.len(), 44);
        let vector = |offset| Vec3::new(values[offset], values[offset + 1], values[offset + 2]);
        let quality = if values[0] == 1024.0 {
            WorldShadowQuality::UnitsLow
        } else {
            WorldShadowQuality::UnitsHigh
        };
        let projection = WorldShadowProjection::primary(quality, vector(1), vector(4), vector(7))?;
        assert_eq!(projection.texture_size(), values[0] as u32);
        assert_eq!(projection.origin(), vector(4));
        assert!(
            (projection.light_direction() - vector(10))
                .abs()
                .max_element()
                < 0.000001
        );
        assert_eq!(projection.receiver_center(), vector(13));
        let actual = projection.caster_view().to_cols_array();
        for (index, (actual, expected)) in actual.into_iter().zip(&values[16..32]).enumerate() {
            assert!(
                (actual - expected).abs() < 0.0015,
                "case {cases} caster {index}: {actual} versus {expected}"
            );
        }
        for (index, (actual, expected)) in projection
            .receiver_rows()
            .into_iter()
            .flat_map(|row| row.to_array())
            .zip(&values[32..])
            .enumerate()
        {
            assert!(
                (actual - expected).abs() < 0.00015,
                "case {cases} receiver {index}: {actual} versus {expected}"
            );
        }
        let near = projection
            .caster_projection()
            .project_point3(Vec3::new(0.0, 0.0, 1.0));
        let far = projection
            .caster_projection()
            .project_point3(Vec3::new(0.0, 0.0, 4000.0));
        assert!(near.z.abs() < 0.000001);
        assert!((far.z - 1.0).abs() < 0.000001);
        cases += 1;
    }
    assert_eq!(cases, 24);
    Ok(())
}

#[test]
fn shadow_quality_retains_native_texture_and_shader_modes() -> Result<(), Box<dyn std::error::Error>>
{
    for (value, size, mode) in [
        (0, None, 0),
        (1, Some(1024), 1),
        (2, Some(2048), 1),
        (3, Some(1024), 2),
        (4, Some(2048), 2),
        (5, Some(2048), 3),
    ] {
        let quality = WorldShadowQuality::from_cvar(value).ok_or("stock quality")?;
        assert_eq!(quality.texture_size(), size);
        assert_eq!(quality.shader_mode(), mode);
    }
    assert!(WorldShadowQuality::from_cvar(6).is_none());
    assert_eq!(
        WorldShadowProjection::primary(
            WorldShadowQuality::Disabled,
            Vec3::ZERO,
            Vec3::ZERO,
            -Vec3::Z
        ),
        Err(WorldShadowProjectionError::Disabled)
    );
    assert_eq!(
        WorldShadowProjection::primary(
            WorldShadowQuality::UnitsHigh,
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::ZERO
        ),
        Err(WorldShadowProjectionError::DegenerateDirection)
    );
    Ok(())
}
