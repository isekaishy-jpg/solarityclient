//! Validates one stock M2's resolved SKIN and animated geometry bounds.

use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;

use glam::{Mat4, Vec3};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedM2Model, Locale,
};
use solarity_rendering::{
    M2AnimationClock, M2BonePose, M2MaterialPose, M2MeshPlan, M2ParticlePose, M2ShaderPlan,
    sample_m2_camera_frame,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os();
    let _executable = arguments.next();
    let data_root = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(usage_error)?;
    let locale = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage_error)?
        .parse::<Locale>()?;
    let model_path = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage_error)?;
    let animation_time_ms = arguments
        .next()
        .map(|value| {
            value
                .into_string()
                .map_err(|_| usage_error())?
                .parse::<f32>()
                .map_err(|_| usage_error())
        })
        .transpose()?
        .unwrap_or(0.0);
    if arguments.next().is_some() {
        return Err(usage_error().into());
    }

    let catalog = ArchiveCatalog::discover(ClientDataRoot::new(data_root)?, locale)?;
    let mut store = AssetStore::mount(catalog)?;
    let model = DecodedM2Model::load(&mut store, &AssetPath::new(model_path)?)?;
    let plan = M2MeshPlan::prepare(&model, 0)?;
    let pose = M2BonePose::compose_with_model_view(
        model.animations(),
        M2AnimationClock::new(0, animation_time_ms, animation_time_ms),
        Mat4::IDENTITY,
    )?;

    let mut minimum = Vec3::splat(f32::INFINITY);
    let mut maximum = Vec3::splat(f32::NEG_INFINITY);
    let mut invalid_vertices = 0_usize;
    let mut non_unit_weights = 0_usize;
    let mut skinned_positions = Vec::with_capacity(plan.vertices().len());
    for vertex in plan.vertices() {
        let source = Vec3::from_array(vertex.position());
        let weights = vertex.bone_weights();
        let indices = vertex.bone_indices();
        let weight_sum = weights.iter().map(|weight| u32::from(*weight)).sum::<u32>();
        if weight_sum != 0 && weight_sum != 255 {
            non_unit_weights += 1;
        }
        let position = if weight_sum == 0 {
            source
        } else {
            let mut position = Vec3::ZERO;
            for influence in 0..4 {
                if weights[influence] == 0 {
                    continue;
                }
                let transform = pose
                    .transforms()
                    .get(usize::from(indices[influence]))
                    .ok_or_else(|| {
                        format!(
                            "vertex references bone {}, but only {} transforms exist",
                            indices[influence],
                            pose.transforms().len()
                        )
                    })?;
                position +=
                    transform.transform_point3(source) * (f32::from(weights[influence]) / 255.0);
            }
            position
        };
        if !position.is_finite() {
            invalid_vertices += 1;
            skinned_positions.push(Vec3::ZERO);
            continue;
        }
        skinned_positions.push(position);
        minimum = minimum.min(position);
        maximum = maximum.max(position);
    }

    let bounds = model.bounds();
    println!("model={}", model.path());
    println!("animation_time_ms={animation_time_ms}");
    println!(
        "vertices={} indices={} draws={} bones={} sequences={} authored_skin_profiles={} attachments={} cameras={}",
        plan.vertices().len(),
        plan.indices().len(),
        plan.draws().len(),
        pose.transforms().len(),
        model.animations().sequences().len(),
        model.skin_profile_count(),
        model.animations().attachments().len(),
        model.animations().cameras().len(),
    );
    println!(
        "authored_bounds={:?}..{:?} radius={}",
        bounds.minimum(),
        bounds.maximum(),
        bounds.sphere_radius()
    );
    println!(
        "skinned_bounds={minimum:?}..{maximum:?} extent={:?} invalid_vertices={invalid_vertices} non_unit_weights={non_unit_weights}",
        maximum - minimum,
    );
    let mut maximum_edge = 0.0_f32;
    let mut long_edges = 0_usize;
    let authored_extent = (bounds.maximum() - bounds.minimum()).length();
    let (triangles, trailing_indices) = plan.indices().as_chunks::<3>();
    debug_assert!(trailing_indices.is_empty());
    for triangle in triangles {
        let points = [
            skinned_positions[usize::from(triangle[0])],
            skinned_positions[usize::from(triangle[1])],
            skinned_positions[usize::from(triangle[2])],
        ];
        for (left, right) in [(0, 1), (1, 2), (2, 0)] {
            let length = points[left].distance(points[right]);
            maximum_edge = maximum_edge.max(length);
            if length > authored_extent * 0.5 {
                long_edges += 1;
            }
        }
    }
    let nonzero_levels = model.skins()[0]
        .submeshes()
        .iter()
        .filter(|submesh| submesh.level != 0)
        .count();
    println!(
        "topology_maximum_edge={maximum_edge} long_edges={long_edges} nonzero_submesh_levels={nonzero_levels}"
    );
    for (texture_index, texture) in model.textures().iter().enumerate() {
        println!(
            "texture={texture_index} kind={:?} flags={:#06X} filename={:?}",
            texture.kind(),
            texture.flags(),
            texture.filename(),
        );
    }
    for (draw_index, draw) in plan.draws().iter().enumerate() {
        let bindings = draw
            .texture_bindings()
            .iter()
            .map(|binding| {
                format!(
                    "{}:{}@{}",
                    binding.stage(),
                    binding.texture_index(),
                    binding.texture_coordinate(),
                )
            })
            .collect::<Vec<_>>();
        println!(
            "draw={draw_index} geoset={} indices={} shader={:#06X} priority={} layer={} blend={:?} flags={:#06X} transparent={} bindings={bindings:?}",
            draw.geoset_id(),
            draw.index_count(),
            draw.batch().shader_id,
            draw.batch().priority_plane,
            draw.batch().material_layer,
            draw.material().blend_mode(),
            draw.material().flags(),
            draw.transparent_sort_unit(),
        );
        let shader = M2ShaderPlan::resolve(&model, draw)?;
        println!(
            "  effect: vertex={:?} pixel={:?} resolved={:#06X}",
            shader.vertex_shader(),
            shader.pixel_shader(),
            shader.resolved_shader_id(),
        );
        let start = M2MaterialPose::sample(
            &model,
            &plan,
            draw_index,
            M2AnimationClock::new(0, 0.0, 0.0),
        )?;
        let current = M2MaterialPose::sample(
            &model,
            &plan,
            draw_index,
            M2AnimationClock::new(0, animation_time_ms, animation_time_ms),
        )?;
        if start != current {
            let start_origins = start
                .texture_transforms()
                .map(|transform| transform.transform_point3(Vec3::ZERO).truncate());
            let current_origins = current
                .texture_transforms()
                .map(|transform| transform.transform_point3(Vec3::ZERO).truncate());
            println!(
                "  animated material: color {:?} -> {:?}, texture origins {:?} -> {:?}",
                start.mesh_color(),
                current.mesh_color(),
                start_origins,
                current_origins,
            );
        }
    }
    for (camera_index, camera) in model.animations().cameras().iter().enumerate() {
        let frame = sample_m2_camera_frame(
            model.animations(),
            camera_index,
            M2AnimationClock::new(0, animation_time_ms, animation_time_ms),
            16.0 / 9.0,
        )?;
        println!(
            "camera={camera_index} diagonal_fov={} near={} far={} position={:?} target={:?}",
            camera.field_of_view_radians(),
            camera.near_clip(),
            camera.far_clip(),
            frame.camera().position(),
            frame.camera().target(),
        );
    }
    for attachment in model.animations().attachments() {
        let transform = pose
            .attachment_transform(
                model.animations(),
                attachment,
                M2AnimationClock::new(0, animation_time_ms, animation_time_ms),
                Mat4::IDENTITY,
            )?
            .map(|transform| transform.to_cols_array());
        println!(
            "attachment={} bone={} position={:?} transform={transform:?}",
            attachment.id(),
            attachment.bone_index(),
            attachment.position()
        );
    }
    for (particle_index, particle) in model.animations().particles().iter().enumerate() {
        let particle_pose = M2ParticlePose::sample(
            model.animations(),
            particle,
            M2AnimationClock::new(0, animation_time_ms, animation_time_ms),
        )?;
        let textures = particle
            .texture_indices()
            .into_iter()
            .flatten()
            .map(|texture_index| {
                model
                    .textures()
                    .get(usize::from(texture_index))
                    .and_then(|texture| texture.filename())
                    .map_or_else(|| format!("#{texture_index}"), ToString::to_string)
            })
            .collect::<Vec<_>>();
        println!(
            "particle={particle_index} id={} type={} blend={} flags={:#010X} priority={} position={:?} atlas={}x{} speed={} gravity={} life={} rate={} area={}x{} textures={textures:?}",
            particle.id(),
            particle.emitter_type(),
            particle.blending_type(),
            particle.flags(),
            particle.priority_plane(),
            particle.position(),
            particle.texture_columns(),
            particle.texture_rows(),
            particle_pose.emission_speed(),
            particle_pose.gravity(),
            particle_pose.lifespan(),
            particle_pose.emission_rate(),
            particle_pose.emission_area_length(),
            particle_pose.emission_area_width(),
        );
    }
    Ok(())
}

fn usage_error() -> IoError {
    IoError::new(
        ErrorKind::InvalidInput,
        "usage: validate_m2_geometry <Data directory> <locale> <M2 path> [animation time ms]",
    )
}
