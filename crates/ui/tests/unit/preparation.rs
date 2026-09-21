//! Real Lua publications match serial preparation; every loan returns on failure.

use crate::test_support as support;

use super::*;
use crate::{FontError, FontSystem, FontWork, FontWorkExecutor, FontWorkOutput};
use solarity_asset::{ArchiveCatalog, AssetError, ClientDataRoot, Locale};
use solarity_cpu::{
    CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuServiceHandle, CpuStoragePlan,
};
use std::{
    cell::Cell,
    error::Error,
    num::NonZeroUsize,
    sync::{Arc, Mutex, mpsc},
};
use support::{Fixture, FixtureFile};

struct Host {
    cpu: CpuServiceHandle,
    reader: Arc<Mutex<AssetStore>>,
    completed: Cell<usize>,
    failure: Cell<u8>,
}

impl FontWorkExecutor for Host {
    fn execute(&self, work: FontWork) -> Result<FontWorkOutput, FontError> {
        assert!(!solarity_cpu::is_worker_thread());
        let reader = self.reader.clone();
        let task = self
            .cpu
            .try_submit_prepared(move || {
                move |_: &solarity_cpu::JobContext<'_>| {
                    assert!(solarity_cpu::is_worker_thread());
                    work.run(
                        &mut reader
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner),
                    )
                }
            })
            .map_err(AssetError::from)?;
        let result = task.join().map_err(AssetError::from)?;
        self.completed.set(self.completed.get() + 1);
        match self.failure.get() {
            1 => {
                return Err(FontError::Execution {
                    message: "native wait failure".into(),
                });
            }
            2 => panic!("host unwind after reclamation"),
            _ => (),
        }
        result
    }
}

fn pool() -> Result<CpuExecutor, solarity_cpu::CpuError> {
    CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::MIN,
        CpuStoragePlan::new(16 << 20, 64 << 20, 16 << 20),
    ))
}

fn host(cpu: &CpuExecutor, catalog: ArchiveCatalog) -> Result<Rc<Host>, AssetError> {
    Ok(Rc::new(Host {
        cpu: cpu.service_handle(),
        reader: Arc::new(Mutex::new(AssetStore::mount(catalog)?)),
        completed: Cell::new(0),
        failure: Cell::new(0),
    }))
}

#[test]
fn native_loans_restore_owned_buffers_on_admission_failure_native_failure_and_panics()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let cpu = pool()?;
    let host = host(&cpu, catalog.clone())?;
    let fonts = FontSystem::with_executor(host.clone());
    let mut assets = AssetStore::mount(catalog)?;
    let mut state = Vec::with_capacity(8);
    state.push(7);
    let address = state.as_ptr();
    let (release, wait) = mpsc::channel();
    let blocker = cpu.try_submit(move || wait.recv())?;
    assert!(
        fonts
            .prepare(&mut assets, &mut state, |state, _, _| state.push(99))
            .is_err()
    );
    assert_eq!(state, [7]);
    assert_eq!(state.as_ptr(), address);
    release.send(())?;
    blocker.join()??;
    let result = fonts.prepare(&mut assets, &mut state, |state, _, _| {
        state.push(11);
        Err::<(), _>("domain error")
    })?;
    assert_eq!(result, Err("domain error"));
    host.failure.set(1);
    assert!(
        fonts
            .prepare(&mut assets, &mut state, |state, _, _| state.push(13))
            .is_err()
    );
    host.failure.set(2);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = fonts.prepare(&mut assets, &mut state, |state, _, _| state.push(17));
        }))
        .is_err()
    );
    host.failure.set(0);
    assert!(
        fonts
            .prepare(&mut assets, &mut state, |state, _, _| {
                state.push(19);
                panic!("worker failure after a private mutation");
            })
            .is_err()
    );
    assert_eq!(state, [7, 11, 13, 17, 19]);
    assert_eq!(state.as_ptr(), address);
    assert_eq!(cpu.snapshot()?.in_flight(), 0);
    assert_eq!(cpu.try_submit(|| 42)?.join()?, 42);
    Ok(())
}

#[test]
fn worker_startup_and_live_publications_match_serial_lua_geometry_and_meshes()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile { path: "Interface/GlueXML/GlueXML.toc", bytes: b"Test.xml\n" },
        FixtureFile { path: "Fonts/Test.ttf", bytes: include_bytes!("../fixtures/tooltip_fixture.ttf") },
        FixtureFile { path: "Interface/GlueXML/Test.xml", bytes: br#"<Ui>
<Font name="TestFont" font="Fonts\Test.ttf"><FontHeight><AbsValue val="16"/></FontHeight></Font>
<Frame name="Root"><Size x="600" y="500"/><Anchors><Anchor point="CENTER"/></Anchors>
<Layers><Layer><FontString name="Label" inherits="TestFont" text="abc"><Size x="100" y="30"/><Anchors><Anchor point="TOPLEFT"/></Anchors></FontString>
<Texture name="Texture"><Size x="100" y="50"/><Anchors><Anchor point="BOTTOMLEFT"/></Anchors><Color r="1" g="0.5" b="0.2"/></Texture></Layer></Layers>
<Frames><EditBox name="Entry" autoFocus="false"><Size x="150" y="30"/><Anchors><Anchor point="CENTER"/></Anchors>
<Scripts><OnLoad>self:SetFontObject(TestFont); self:SetText("abc")</OnLoad></Scripts></EditBox>
<SimpleHTML name="Document" font="TestFont"><Size x="180" y="20"/><Anchors><Anchor point="BOTTOMRIGHT"/></Anchors><Scripts><OnLoad>self:SetText("abc")</OnLoad></Scripts></SimpleHTML></Frames>
<Scripts><OnUpdate>if COMMAND then HOST_CHECK(); local command = COMMAND; COMMAND = nil; assert(loadstring(command))() end</OnUpdate></Scripts>
</Frame></Ui>"# },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let cpu = pool()?;
    let host = host(&cpu, catalog.clone())?;
    let start = |fonts| -> Result<GlueManager, Box<dyn Error>> {
        let assets = AssetStoreHandle::new(AssetStore::mount(catalog.clone())?);
        let environment = UiScriptEnvironment::new(1280, 720, false)?
            .with_shared_asset_store(assets.clone())
            .with_font_system(fonts);
        let manager =
            GlueManager::start_shared_owner(assets, environment, UiManifestKind::Glue, None)?;
        let check = manager.bundle().lua().create_function(|_, ()| {
            assert!(!solarity_cpu::is_worker_thread());
            Ok(())
        })?;
        manager.bundle().lua().globals().set("HOST_CHECK", check)?;
        Ok(manager)
    };
    let mut worker = start(FontSystem::with_executor(host.clone()))?;
    let mut serial = start(FontSystem::new()?)?;
    compare(&worker, &serial);
    let html_index = worker
        .objects()
        .iter()
        .position(|object| object.name() == Some("Document"))
        .ok_or("HTML owner")?;
    let frozen_html = worker.simple_html().clone();
    let frozen_node = frozen_html.node(html_index).ok_or("HTML node")?;
    assert!(std::ptr::eq(
        frozen_node,
        worker.simple_html().node(html_index).ok_or("HTML node")?
    ));
    let original_text = frozen_node
        .lines()
        .iter()
        .map(|line| line.text().to_owned())
        .collect::<Vec<_>>();
    assert!(host.completed.get() >= 2);
    let initial = host.completed.get();
    for command in [
        "Document:SetText('abcdef')",
        "Label:SetText('abcdef'); Entry:SetText('abcde'); Entry:SetCursorPosition(2); Entry:HighlightText(1, 3)",
        "Root:SetWidth(520); Label:SetPoint('TOPLEFT', Root, 'TOPLEFT', 11, -7)",
        "Root:SetAlpha(0.4)",
        "Label:Hide()",
        "Label:Show(); Root:SetAlpha(1)",
        "local text = Root:CreateFontString('Later', 'OVERLAY'); text:SetFontObject(TestFont); text:SetPoint('CENTER'); text:SetText('abc')",
        "Root:SetFrameStrata('HIGH'); Root:SetFrameLevel(8)",
        "Entry:SetText('a'); Entry:SetCursorPosition(1)",
        "Root:SetScale(0.8); Texture:SetVertexColor(0.2, 0.5, 1)",
    ] {
        for manager in [&mut worker, &mut serial] {
            manager.bundle().lua().globals().set("COMMAND", command)?;
            manager.update(0.0)?;
            assert!(manager.take_callback_failure().is_none(), "{command}");
        }
        compare(&worker, &serial);
    }
    assert!(host.completed.get() > initial);
    assert_eq!(
        frozen_node
            .lines()
            .iter()
            .map(|line| line.text().to_owned())
            .collect::<Vec<_>>(),
        original_text
    );
    assert_ne!(
        worker
            .simple_html()
            .node(html_index)
            .ok_or("HTML node")?
            .lines()
            .iter()
            .map(|line| line.text().to_owned())
            .collect::<Vec<_>>(),
        original_text
    );
    assert_eq!(cpu.snapshot()?.in_flight(), 0);
    Ok(())
}

fn compare(worker: &GlueManager, serial: &GlueManager) {
    assert_eq!(worker.native.live, serial.native.live);
    assert_eq!(
        worker.geometry().region_count(),
        serial.geometry().region_count()
    );
    for index in 0..worker.geometry().region_count() {
        assert_eq!(
            worker.geometry().region(index),
            serial.geometry().region(index)
        );
    }
    assert_eq!(worker.glyphs().rgba8(), serial.glyphs().rgba8());
    assert_eq!(
        worker.glyphs().quads(worker.geometry()),
        serial.glyphs().quads(serial.geometry())
    );
    assert_eq!(
        worker.render_plan().mesh().vertices(),
        serial.render_plan().mesh().vertices()
    );
    assert_eq!(
        worker.render_plan().mesh().indices(),
        serial.render_plan().mesh().indices()
    );
    assert_eq!(
        worker.render_plan().mesh().object_indices(),
        serial.render_plan().mesh().object_indices()
    );
}
