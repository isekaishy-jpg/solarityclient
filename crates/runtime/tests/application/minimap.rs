//! Archive-to-GPU coverage for native FrameXML minimap composition.

use std::error::Error;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use glam::Vec3;
use solarity_asset::{AssetStoreHandle, ClientDataRoot, Locale, MapCatalog};
use solarity_cpu::CpuPoolConfig;
use solarity_rendering::VulkanBootstrap;
use solarity_ui::{AddonCatalog, UiScriptEnvironment};
use wow_wdt::{WdtFile, WdtWriter, version::WowVersion};

use super::*;
use crate::application::login_ui::RuntimeUiResidency;
use crate::platform::SdlPlatform;
use crate::test_support::ClientFixture;
use crate::{WindowConfiguration, WindowMode};

#[test]
fn minimap_streams_into_native_order_and_reuses_gpu_storage() -> Result<(), Box<dyn Error>> {
    let _sdl_guard = crate::test_support::SDL_TEST_LOCK
        .lock()
        .map_err(|_| "SDL test lock poisoned")?;
    let mut map_fields = [0; 66];
    map_fields[1] = 1;
    let map_bytes = dbc(&map_fields, 66, b"\0Test\0");
    let mut wdt = Vec::new();
    WdtWriter::new(&mut wdt).write(&WdtFile::new(WowVersion::WotLK))?;
    let coefficients = dbc(&[0; 1100], 1, b"\0");
    let base = dbc(&[0; 11], 1, b"\0");
    let slots = dbc(&[], 3, b"\0");
    let red = blp(&[0xffff0000; 16], 4);
    let green = blp(&[0xff00ff00; 16], 4);
    let mask_pixels = (0..64)
        .flat_map(|y| {
            (0..64).map(move |x| {
                let distance = (x as f32 - 31.5).powi(2) + (y as f32 - 31.5).powi(2);
                if distance < 31.0 * 31.0 {
                    0xffffffff
                } else {
                    0x00ffffff
                }
            })
        })
        .collect::<Vec<_>>();
    let mask = blp(&mask_pixels, 64);
    let fixture = ClientFixture::with_common_files(&[
        ("DBFilesClient/Map.dbc", &map_bytes),
        ("World/Maps/Test/Test.wdt", &wdt),
        ("DBFilesClient/gtChanceToMeleeCritBase.dbc", &base),
        ("DBFilesClient/gtChanceToSpellCritBase.dbc", &base),
        ("DBFilesClient/gtChanceToMeleeCrit.dbc", &coefficients),
        ("DBFilesClient/gtChanceToSpellCrit.dbc", &coefficients),
        ("DBFilesClient/gtOCTRegenHP.dbc", &coefficients),
        ("DBFilesClient/gtRegenHPPerSpt.dbc", &coefficients),
        ("DBFilesClient/gtRegenMPPerSpt.dbc", &coefficients),
        ("DBFilesClient/PaperDollItemFrame.dbc", &slots),
        ("Textures/Minimap/md5translate.trs", b"Test/map31_31.blp\ta.blp\nTest/map32_31.blp\tb.blp\nTest/map32_32.blp\tc.blp\nTest/map31_32.blp\td.blp\n"),
        ("Textures/Minimap/a.blp", &red), ("Textures/Minimap/b.blp", &red),
        ("Textures/Minimap/c.blp", &red), ("Textures/Minimap/d.blp", &red),
        ("Textures/MinimapMask.blp", &mask),
        ("Interface/Minimap/MinimapArrow.blp", &green),
        ("Interface/FrameXML/FrameXML.toc", b"Map.xml\n"),
        ("Interface/FrameXML/Map.xml", br#"<Ui>
<Frame name="Backdrop"><Size x="768" y="768"/><Anchors><Anchor point="CENTER"/></Anchors>
<Layers><Layer><Texture setAllPoints="true"><Color r="1" g="0" b="1"/></Texture></Layer></Layers></Frame>
<Minimap name="Minimap" frameLevel="2"><Size x="512" y="512"/><Anchors><Anchor point="CENTER"/></Anchors>
<Layers><Layer level="ARTWORK"><Texture><Size x="64" y="64"/><Anchors><Anchor point="CENTER" x="128"/></Anchors><Color r="0" g="0" b="1"/></Texture></Layer></Layers>
<Scripts><OnLoad>self:SetPlayerTextureWidth(32); self:SetPlayerTextureHeight(32)</OnLoad></Scripts></Minimap></Ui>"#),
        ("Interface/FrameXML/Bindings.xml", br#"<Bindings>
<Binding name="HIDE">Minimap:Hide(); Minimap:SetWidth(520)</Binding><Binding name="SHOW">Minimap:Show(); Minimap:SetWidth(512)</Binding>
<Binding name="ROTATE">SetCVar("rotateMinimap", "1")</Binding>
<Binding name="MISSING">Minimap:SetPlayerTexture("Interface/Minimap/Missing")</Binding>
</Bindings>"#),
        ("WTF/DefaultBindings.wtf", b""),
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog.clone())?;
    let maps = MapCatalog::load(&mut store)?;
    let map = TerrainMap::load(&mut store, maps.map(0).ok_or("missing map")?)?;
    let mut scene = RuntimeMinimapScene::new(&mut store, catalog)?;
    let mut manager = FrameManager::start_shared(
        AssetStoreHandle::new(store),
        UiScriptEnvironment::new(96, 96, false)?,
        &[],
        &AddonCatalog::default(),
    )?;
    let platform = SdlPlatform::start(WindowConfiguration::new(96, 96, WindowMode::Windowed))?;
    let mut renderer = renderer(&platform)?;
    let mut cache = solarity_asset::BlpTextureCache::new();
    let mut residency = RuntimeUiResidency::new();
    let mut ui =
        RuntimeUiFrame::prepare_frame(&mut renderer, &manager, &mut cache, &mut residency)?;
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(NonZeroUsize::MIN, NonZeroUsize::MIN))?;
    let (release, wait) = std::sync::mpsc::channel();
    let blocker = cpu.try_submit(move || wait.recv())?;
    let world = Some(WorldTransform::new(Vec3::ZERO, 0.0));
    scene.synchronize(&mut renderer, &cpu, &manager, &ui, 0, Some(&map), world)?;
    assert!(!scene.ready());
    assert!(scene.request_deferred);
    assert!(
        scene.slots[0].frame.is_none(),
        "unresolved images must not upload an empty mesh"
    );
    assert_eq!(scene.draws(&ui).len(), ui.draws().len());
    release.send(())?;
    blocker.join()??;
    wait_ready(
        &mut scene,
        &mut renderer,
        &cpu,
        &manager,
        &ui,
        &map,
        0,
        world,
    )?;
    assert_eq!(scene.textures.len(), 6);
    assert_eq!(scene.draws(&ui).len(), 7);
    let retained_mesh = scene.slots[0]
        .frame
        .as_ref()
        .ok_or("missing minimap GPU mesh")?
        .mesh();
    let pixels = capture(&mut renderer, scene.draws(&ui))?;
    let mesh_generation = renderer
        .ui_mesh_info(retained_mesh)
        .ok_or("missing mesh info")?;
    // A surrounding UI refresh must not touch unchanged minimap vertices.
    scene.synchronize(&mut renderer, &cpu, &manager, &ui, 1, Some(&map), world)?;
    assert_eq!(
        renderer
            .ui_mesh_info(retained_mesh)
            .ok_or("lost mesh info")?,
        mesh_generation
    );
    assert_eq!(pixel(&pixels, 48, 32), [255, 0, 0]);
    assert_eq!(pixel(&pixels, 48, 48), [0, 255, 0]);
    assert_eq!(
        pixel(&pixels, 64, 48),
        [0, 0, 255],
        "authored ARTWORK must cover native tiles"
    );
    assert_eq!(
        pixel(&pixels, 18, 18),
        [255, 0, 255],
        "map mask must preserve the background"
    );

    manager.invoke_binding("ROTATE", true)?;
    ui.refresh_frame(&mut renderer, &manager, &mut cache, &mut residency)?;
    let moved = Some(WorldTransform::new(
        Vec3::new(40.0, 35.0, 0.0),
        std::f32::consts::FRAC_PI_2,
    ));
    scene.synchronize(&mut renderer, &cpu, &manager, &ui, 1, Some(&map), moved)?;
    assert_eq!(
        scene.slots[0]
            .frame
            .as_ref()
            .ok_or("lost moving map")?
            .mesh(),
        retained_mesh
    );
    assert_eq!(scene.textures.len(), 6);
    manager.invoke_binding("HIDE", true)?;
    ui.refresh_frame(&mut renderer, &manager, &mut cache, &mut residency)?;
    scene.synchronize(&mut renderer, &cpu, &manager, &ui, 2, Some(&map), moved)?;
    assert_eq!(scene.draws(&ui).len(), 1);
    manager.invoke_binding("SHOW", true)?;
    ui.refresh_frame(&mut renderer, &manager, &mut cache, &mut residency)?;
    scene.synchronize(&mut renderer, &cpu, &manager, &ui, 3, Some(&map), moved)?;
    assert_eq!(scene.draws(&ui).len(), 7);
    assert_eq!(
        scene.slots[0]
            .frame
            .as_ref()
            .ok_or("lost revealed map")?
            .mesh(),
        retained_mesh
    );

    let unmapped = Some(WorldTransform::new(Vec3::new(5000.0, 5000.0, 0.0), 0.0));
    scene.synchronize(&mut renderer, &cpu, &manager, &ui, 3, Some(&map), unmapped)?;
    assert_eq!(
        scene.draws(&ui).len(),
        3,
        "a new location must not display stale tiles"
    );
    manager.invoke_binding("MISSING", true)?;
    ui.refresh_frame(&mut renderer, &manager, &mut cache, &mut residency)?;
    wait_ready(
        &mut scene,
        &mut renderer,
        &cpu,
        &manager,
        &ui,
        &map,
        4,
        unmapped,
    )?;
    assert_eq!(scene.draws(&ui).len(), 2);
    assert!(
        scene
            .failed
            .contains(&AssetPath::new("Interface/Minimap/Missing.blp")?)
    );
    scene.synchronize(&mut renderer, &cpu, &manager, &ui, 4, Some(&map), unmapped)?;
    assert!(
        scene.pending.is_none(),
        "missing textures must not retry every frame"
    );
    cpu.shutdown()?;
    renderer.shutdown()?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn wait_ready(
    scene: &mut RuntimeMinimapScene,
    renderer: &mut VulkanRenderer,
    cpu: &CpuExecutor,
    manager: &FrameManager,
    ui: &RuntimeUiFrame,
    map: &TerrainMap,
    revision: u64,
    world: Option<WorldTransform>,
) -> Result<(), Box<dyn Error>> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        scene.synchronize(renderer, cpu, manager, ui, revision, Some(map), world)?;
        if scene.ready() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("minimap worker did not finish".into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[allow(unsafe_code)]
fn renderer(platform: &SdlPlatform) -> Result<VulkanRenderer, Box<dyn Error>> {
    let bootstrap = VulkanBootstrap::start(&platform.vulkan_instance_extensions()?)?;
    // SAFETY: The platform owns the live window and outlives this renderer.
    let surface = unsafe { platform.create_vulkan_surface(bootstrap.instance_handle()) }?;
    Ok(unsafe { bootstrap.attach_surface(surface, platform.pixel_extent(), 0) }?)
}

fn capture(
    renderer: &mut VulkanRenderer,
    draws: &[UiPreparedDraw],
) -> Result<Vec<u8>, Box<dyn Error>> {
    renderer.request_frame_capture()?;
    renderer.present_ui([768.0; 2], draws)?;
    Ok(renderer
        .take_captured_frame()?
        .ok_or("missing capture")?
        .rgba8()
        .to_vec())
}

fn pixel(bytes: &[u8], x: usize, y: usize) -> [u8; 3] {
    let start = (y * 96 + x) * 4;
    [bytes[start], bytes[start + 1], bytes[start + 2]]
}

fn dbc(fields: &[u32], width: u32, strings: &[u8]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [
        fields.len() as u32 / width,
        width,
        width * 4,
        strings.len() as u32,
    ] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for value in fields {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(strings);
    bytes
}

fn blp(pixels: &[u32], width: u32) -> Vec<u8> {
    let mut bytes = b"BLP2".to_vec();
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&[3, 8, 8, 0]);
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&1172_u32.to_le_bytes());
    bytes.resize(84, 0);
    bytes.extend_from_slice(&(pixels.len() as u32 * 4).to_le_bytes());
    bytes.resize(1172, 0);
    for pixel in pixels {
        bytes.extend_from_slice(&pixel.to_le_bytes());
    }
    bytes
}
