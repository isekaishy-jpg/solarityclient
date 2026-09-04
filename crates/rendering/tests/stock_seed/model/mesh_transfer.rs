//! GPU lifetime coverage for geometry admitted between completed scene revisions.

#![allow(unsafe_code)]

use std::error::Error;

use glam::{Mat4, Vec3, Vec4};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedM2Model, Locale,
};
use solarity_rendering::{
    M2LocalLightCount, M2LocalLightState, M2MaterialUniform, M2MeshPlan, M2SampledTexture,
    M2SceneUniform, M2ShaderPermutation, M2ShaderPlan, M2ShadowFiltering, M2ShadowPermutation,
    M2TextureSet, VulkanBootstrap,
};

use crate::support::{Fixture, FixtureFile};

/// New geometry must be drawable immediately after admission while older frames
/// are in flight. A final unused admission must also retire safely at shutdown.
#[test]
fn new_meshes_present_in_submission_order_and_retire_without_a_final_draw()
-> Result<(), Box<dyn Error>> {
    let model_bytes = super::render_m2_bytes("Streamed", 1)?;
    let skin_bytes = super::render_skin_bytes()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Creature\\Solarity\\First.m2",
            bytes: &model_bytes,
        },
        FixtureFile {
            path: "Creature\\Solarity\\First00.skin",
            bytes: &skin_bytes,
        },
        FixtureFile {
            path: "Creature\\Solarity\\Second.m2",
            bytes: &model_bytes,
        },
        FixtureFile {
            path: "Creature\\Solarity\\Second00.skin",
            bytes: &skin_bytes,
        },
        FixtureFile {
            path: "Creature\\Solarity\\Unused.m2",
            bytes: &model_bytes,
        },
        FixtureFile {
            path: "Creature\\Solarity\\Unused00.skin",
            bytes: &skin_bytes,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let mut models = Vec::new();
    let mut plans = Vec::new();
    for name in ["First", "Second", "Unused"] {
        let path = AssetPath::new(format!("Creature\\Solarity\\{name}.m2"))?;
        let model = DecodedM2Model::load(&mut store, &path)?;
        plans.push(M2MeshPlan::prepare(&model, 0)?);
        models.push(model);
    }

    let _sdl_test = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity deferred mesh test", 64, 64)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The bootstrap enabled this live window's required extensions.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: The renderer receives sole ownership of the surface from this instance.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    let draw = &plans[0].draws()[0];
    let shader = M2ShaderPlan::resolve(&models[0], draw)?;
    let permutation = M2ShaderPermutation::resolve(
        draw,
        M2LocalLightCount::Zero,
        M2ShadowPermutation::Disabled,
        M2ShadowFiltering::Direct,
    );
    let pipeline = renderer.prepare_m2_pipeline(shader, permutation)?;
    let white = renderer.upload_stock_m2_white()?;
    let sampler = renderer.prepare_m2_sampler(&models[0].textures()[0])?;
    let textures = renderer.prepare_m2_texture_sets(&[M2TextureSet::Two(
        [M2SampledTexture::new(white, sampler); 2],
    )])?[0];
    let material = M2MaterialUniform::new(
        Mat4::IDENTITY,
        [Mat4::IDENTITY; 2],
        Mat4::IDENTITY,
        Vec4::ONE,
        Vec4::ZERO,
        Vec4::new(0.5, 0.0, 0.0, 0.0),
    );
    let scene = M2SceneUniform::new(
        Mat4::IDENTITY,
        Vec3::ZERO,
        Vec3::ONE,
        Vec3::ZERO,
        Vec3::Z,
        Vec4::ZERO,
        Vec3::ZERO,
        [M2LocalLightState::disabled(); 4],
    );
    let mut handles = Vec::new();
    for plan in &plans[..2] {
        let handle = renderer.upload_m2_mesh(plan)?;
        assert!(!handles.contains(&handle));
        handles.push(handle);
        let draw =
            renderer.prepare_m2_draw(handle, pipeline, textures, plan, 0, false, material, 0, 0)?;
        let bones = vec![Mat4::IDENTITY; draw.required_bone_transforms()];
        assert_eq!(renderer.present_m2(scene, &bones, &[draw])?.draw_count(), 1);
        assert_eq!(renderer.upload_m2_mesh(plan)?, handle);
    }
    for (plan, handle) in plans.iter().zip(handles) {
        assert_eq!(
            renderer
                .m2_mesh_info(handle)
                .ok_or("mesh was retired with its staging")?
                .path(),
            plan.path()
        );
    }
    // Exercise shutdown with a queued transfer and no subsequent presentation
    // or cache lookup to opportunistically retire its source allocation.
    renderer.upload_m2_mesh(&plans[2])?;
    drop(renderer);
    Ok(())
}
