//! Selected appearance churn yields service without publishing obsolete results.

use super::{
    GlueCharacterWorkerRequest, ResidentGlueCharacterKey, RuntimePlayerError, coalesced_step,
};
use solarity_cpu::{CpuExecutor, CpuPoolConfig, CpuStoragePlan};
use solarity_rendering::CharacterComponentTextureLevel;
use solarity_ui::{
    UiCharacterDirectory, UiCharacterEquipment, UiCharacterInfo, UiCharacterPetPreview,
};
use std::{
    error::Error,
    num::NonZeroUsize,
    ops::ControlFlow,
    sync::{Arc, Mutex, mpsc},
};

/// Public Glue projection supplies complete selection keys without opening UI windows.
fn request(guid: u64) -> GlueCharacterWorkerRequest {
    let directory = UiCharacterDirectory::new(
        vec![UiCharacterInfo::new(
            guid,
            "Soap".into(),
            "Human".into(),
            1,
            "Human".into(),
            "Mage".into(),
            8,
            80,
            None,
            2,
            0,
            [0; 5],
            [UiCharacterEquipment::default(); 23],
            UiCharacterPetPreview::default(),
            0,
            0,
        )],
        "Human".into(),
    );
    GlueCharacterWorkerRequest {
        key: ResidentGlueCharacterKey::Selection(Box::new(
            directory
                .selection_preview()
                .unwrap_or_else(|| unreachable!("the single projected character is selected")),
        )),
        component_texture_level: CharacterComponentTextureLevel::DEFAULT,
    }
}

/// A request replacement during preparation cannot monopolize the only flexible worker.
#[test]
fn appearance_churn_yields_to_queued_service_and_skips_superseded_input()
-> Result<(), Box<dyn Error>> {
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
        NonZeroUsize::new(3).ok_or("capacity")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let (release, wait) = mpsc::channel();
    let blocker = cpu.try_submit(move || wait.recv())?;
    let selected = Arc::new(Mutex::new(Some(request(1))));
    let work = Arc::clone(&selected);
    let (record, order) = mpsc::channel();
    let attempts = record.clone();
    let task = cpu.try_reserve()?.submit_steps(move || {
        coalesced_step(&work, |input| {
            let ResidentGlueCharacterKey::Selection(preview) = &input.key else {
                unreachable!("fixture supplies selection input")
            };
            let guid = preview.guid();
            let _sent = attempts.send(guid);
            if guid == 1 {
                *work
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(request(2));
            }
            Ok(guid)
        })
    });
    let marker = cpu.try_submit(move || {
        let _sent = record.send(99);
        *selected
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(request(3));
    })?;
    release.send(())?;
    blocker.join()??;
    marker.join()?;
    let result = task.join()?.map_err(|error| error.source)?;
    cpu.shutdown()?;
    let (key, prepared) = result.ok_or("missing current result")?;
    assert_eq!(prepared, 3);
    assert!(key.same_residency(&request(3).key));
    assert_eq!(order.into_iter().collect::<Vec<_>>(), [1, 99, 3]);
    Ok(())
}

/// Obsolete failure is discarded, but a current failure keeps its exact request identity.
#[test]
fn appearance_failure_revalidates_current_selection_before_publication() {
    let selected = Mutex::new(Some(request(1)));
    let changed = coalesced_step::<()>(&selected, |_| {
        *selected
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(request(2));
        Err(RuntimePlayerError::MissingGlueCharacterWorkerResult)
    });
    assert!(matches!(changed, ControlFlow::Continue(())));
    let terminal = coalesced_step::<()>(&selected, |_| {
        Err(RuntimePlayerError::MissingGlueCharacterWorkerResult)
    });
    let ControlFlow::Break(Err(failure)) = terminal else {
        panic!("current failure was lost")
    };
    assert!(failure.key.same_residency(&request(2).key));
    assert!(matches!(
        failure.source,
        RuntimePlayerError::MissingGlueCharacterWorkerResult
    ));
}

/// Withdrawal completes the admitted operation and never starts another preparation.
#[test]
fn appearance_withdrawal_ends_the_continuation_before_another_attempt() {
    let selected = Mutex::new(Some(request(1)));
    let withdrawn = coalesced_step(&selected, |_| {
        *selected
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        Ok(7)
    });
    assert!(matches!(withdrawn, ControlFlow::Break(Ok(None))));
    let absent = coalesced_step::<()>(&selected, |_| panic!("withdrawn input cannot prepare"));
    assert!(matches!(absent, ControlFlow::Break(Ok(None))));
}
