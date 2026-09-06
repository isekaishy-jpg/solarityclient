//! Actual GPU coverage of portrait alpha replacement, sampling, and slot reuse.

#![allow(unsafe_code)]

use super::*;
use solarity_rendering::{
    UiMeshPlan, UiPreparedDraw, UiRenderBlend, UiRenderQuad, UiRenderSource, UiSampledTexture,
    UiSamplerInfo, UiShaderSource, UiTextureAddressMode, UiTextureResidency, VulkanRenderer,
};

#[test]
fn portrait_mask_preserves_model_rgb_and_updates_the_same_sampled_image()
-> Result<(), Box<dyn Error>> {
    let mut model_bytes = render_m2_bytes("Portrait", 1)?;
    let vertices = m2_array_offset(&model_bytes, 0x3c)?;
    for (index, position) in [[-1.0_f32, -1.0, 0.5], [3.0, -1.0, 0.5], [-1.0, 3.0, 0.5]]
        .into_iter()
        .enumerate()
    {
        for (axis, value) in position.into_iter().enumerate() {
            let offset = vertices + index * 48 + axis * 4;
            model_bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    let skin = render_skin_bytes()?;
    // Deliberately asymmetric alpha proves both row order and alpha overwrite.
    // Bright green RGB must never replace the model's red or yellow pixels.
    let pixels = (0..64)
        .flat_map(|y| {
            (0..64).map(move |x| {
                let alpha = if y >= 32 {
                    255
                } else if x < 32 {
                    0
                } else {
                    128
                };
                (alpha << 24) | 0x0000_ff00
            })
        })
        .collect::<Vec<_>>();
    let mask_bytes = raw3_blp_pixels(64, 64, &pixels);
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Portrait.m2",
            bytes: &model_bytes,
        },
        FixtureFile {
            path: "Portrait00.skin",
            bytes: &skin,
        },
        FixtureFile {
            path: "Mask.blp",
            bytes: &mask_bytes,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut assets = AssetStore::mount(catalog)?;
    let model = DecodedM2Model::load(&mut assets, &AssetPath::new("Portrait.m2")?)?;
    let plan = M2MeshPlan::prepare(&model, 0)?;
    let mask = BlpTextureSource::load(&mut assets, &AssetPath::new("Mask.blp")?)?;

    let _sdl_test = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity portrait test", 64, 64)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: This bootstrap enabled the live window's required extensions.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: SDL transfers sole surface ownership; the window outlives the renderer.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    let mesh = renderer.upload_m2_mesh(&plan)?;
    let shader = M2ShaderPlan::resolve(&model, &plan.draws()[0])?;
    let permutation = M2ShaderPermutation::resolve(
        &plan.draws()[0],
        M2LocalLightCount::Zero,
        M2ShadowPermutation::Disabled,
        M2ShadowFiltering::Direct,
    );
    let pipeline = renderer.prepare_m2_pipeline(shader, permutation)?;
    let white = renderer.upload_stock_m2_white()?;
    let sampler = renderer.prepare_m2_sampler(&model.textures()[0])?;
    let texture_set = renderer.prepare_m2_texture_sets(&[M2TextureSet::Two(
        [M2SampledTexture::new(white, sampler); 2],
    )])?[0];
    let mask = renderer.upload_blp_texture(&mask, BlpColorSpace::Linear)?;
    let material = M2MaterialUniform::new(
        Mat4::IDENTITY,
        [Mat4::IDENTITY; 2],
        Mat4::IDENTITY,
        Vec4::new(1.0, 0.0, 0.0, 1.0),
        Vec4::ZERO,
        Vec4::new(0.5, 0.0, 0.0, 0.0),
    );
    let red =
        renderer.prepare_m2_draw(mesh, pipeline, texture_set, &plan, 0, false, material, 0, 0)?;
    let yellow_material = M2MaterialUniform::new(
        Mat4::IDENTITY,
        [Mat4::IDENTITY; 2],
        Mat4::IDENTITY,
        Vec4::new(1.0, 1.0, 0.0, 1.0),
        Vec4::ZERO,
        Vec4::new(0.5, 0.0, 0.0, 0.0),
    );
    let yellow = renderer.prepare_m2_draw(
        mesh,
        pipeline,
        texture_set,
        &plan,
        0,
        false,
        yellow_material,
        0,
        0,
    )?;
    let bones = vec![Mat4::IDENTITY; red.required_bone_transforms()];
    let scene = M2SceneUniform::new(
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        Vec3::ZERO,
        Vec3::ONE,
        Vec3::ZERO,
        Vec3::Z,
        Vec4::ZERO,
        Vec3::ZERO,
        [M2LocalLightState::disabled(); 4],
    );
    assert!(renderer.unit_portrait_texture("player").is_none());
    let handle = renderer.render_unit_portrait("player", scene, &bones, &[red], mask)?;
    assert_eq!(renderer.unit_portrait_texture("player"), Some(handle));
    let ui = portrait_ui(&mut renderer, handle)?;
    for (draw, expected_rgb) in [
        (red, [255, 0, 0]),
        (yellow, [255, 255, 0]),
        (red, [255, 0, 0]),
    ] {
        // The UI submission remains in flight when the same image is rewritten.
        renderer.present_ui([64.0, 64.0], &ui)?;
        assert_eq!(
            renderer.render_unit_portrait("player", scene, &bones, &[draw], mask)?,
            handle
        );
        renderer.request_frame_capture()?;
        renderer.present_ui([64.0, 64.0], &ui)?;
        let captured = renderer
            .take_captured_frame()?
            .ok_or("missing portrait capture")?;
        let at = |x, y| rgba8_pixel(captured.rgba8(), captured.extent().0, x, y);
        assert_eq!(&at(16, 16)[..3], &[0, 0, 255], "transparent mask region");
        assert_eq!(
            &at(16, 48)[..3],
            &expected_rgb,
            "opaque mask region preserves model RGB"
        );
        let half = at(48, 16);
        assert!((126..=129).contains(&half[0]), "half alpha red: {half:?}");
        assert!((126..=129).contains(&half[2]), "half alpha blue: {half:?}");
        assert_eq!(half[1] > 100, expected_rgb[1] > 100);
    }
    // A second unit has an independent target, including queued shutdown work.
    let target = renderer.render_unit_portrait("target", scene, &bones, &[yellow], mask)?;
    assert_ne!(handle, target);
    assert_eq!(renderer.unit_portrait_texture("player"), Some(handle));
    Ok(())
}

fn portrait_ui(
    renderer: &mut VulkanRenderer,
    handle: solarity_rendering::UiPortraitTextureHandle,
) -> Result<Vec<UiPreparedDraw>, Box<dyn Error>> {
    let quad = |index, source, color| {
        UiRenderQuad::new(
            index,
            source,
            UiRenderBlend::Alpha,
            UiTextureAddressMode::Clamp,
            UiTextureAddressMode::Clamp,
            UiTextureResidency::Blocking,
            false,
            [0.0, 0.0, 64.0, 64.0],
            [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]],
            [color; 4],
        )
    };
    let plan = UiMeshPlan::prepare(
        [64.0, 64.0],
        [
            quad(0, UiRenderSource::VertexColor, [0.0, 0.0, 1.0, 1.0]),
            quad(
                1,
                UiRenderSource::UnitPortrait("player".to_owned()),
                [1.0; 4],
            ),
        ]
        .into_iter(),
    )?;
    let mesh = renderer.upload_ui_mesh(&plan)?;
    let flat = renderer.prepare_ui_pipeline(UiShaderSource::VertexColor, UiRenderBlend::Alpha)?;
    let textured = renderer.prepare_ui_pipeline(UiShaderSource::Texture, UiRenderBlend::Alpha)?;
    let sampler = renderer.prepare_ui_sampler(UiSamplerInfo::new(
        UiTextureAddressMode::Clamp,
        UiTextureAddressMode::Clamp,
    ))?;
    let set = renderer.prepare_ui_texture_sets(&[UiSampledTexture::portrait(handle, sampler)])?[0];
    Ok(vec![
        renderer.prepare_ui_draw(mesh, flat, None, &plan, 0)?,
        renderer.prepare_ui_draw(mesh, textured, Some(set), &plan, 1)?,
    ])
}
