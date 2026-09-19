//! Actual condition waits are distinct from ready consumption and state reclamation.

use super::FrameBatch;
use super::waiting::WaitTarget;
use crate::{
    CompletionPort, CpuExecutor, CpuPoolConfig, CpuService, CpuStorageClass, CpuStoragePlan,
    FrameBatchPlan, JobOutcome,
};
use solarity_profiling::{Capture, begin_frame};
use std::{
    error::Error,
    num::NonZeroUsize,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

/// A metadata guard controls the real wait boundary, without sleeps or timing
/// thresholds. The prerequisite producer cannot signal this batch until its
/// condition wait releases that guard. Public ready/consume APIs then verify
/// the original instrumentation regression and return-on-unwind contract.
#[test]
fn wait_spans_exclude_ready_probes_callbacks_and_reclamation() -> Result<(), Box<dyn Error>> {
    let root = std::env::temp_dir().join(format!("solarity-batch-waits-{}", std::process::id()));
    let mut capture = Capture::new(&root, "fixture=batch-waits".to_owned());
    let (_, path) = capture.toggle()?;
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::new(2).ok_or("workers")?,
        NonZeroUsize::new(8).ok_or("capacity")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 0),
    ))?;
    let mut frame = Some(begin_frame());
    for (mode, service) in [(1, None), (2, Some(CpuService::Required))] {
        let gate = CompletionPort::new(1, cpu.storage(), CpuStorageClass::Frame)?;
        let mut producer = gate.producer()?;
        let mut batch = FrameBatch::new(|value: &mut usize| *value += 1);
        // LoadBatch selects this exact fixed admission class in its constructor.
        batch.service = service;
        batch.begin_when(&cpu, FrameBatchPlan::new(1, 0), &gate.readiness())?;
        let job = batch.push(&mut Some(41))?;
        batch.close();
        let core = Arc::clone(&batch.core);
        let origin = core.lock().trace;

        origin.value("fixture.pending.begin", mode, 0, 0);
        assert!(batch.try_with_result(&job, |value| *value)?.is_none());
        origin.value("fixture.pending.end", mode, 0, 0);

        let state = core.lock();
        let release = std::thread::spawn(move || producer.complete(JobOutcome::Succeeded));
        let state = core.wait_for_change(state, WaitTarget::Result(job.index));
        drop(state);
        release.join().map_err(|_| "producer panicked")??;
        batch.wait_until_finished()?;

        if service.is_some() {
            // Late main consumption retains the sampled producer's identity.
            drop(frame.take());
            drop(begin_frame());
            assert!(!solarity_profiling::detail_enabled());
        }
        origin.value("fixture.ready.begin", mode, 0, 0);
        assert_eq!(batch.wait_for_outcome(&job)?, JobOutcome::Succeeded);
        batch.wait_until_finished()?;
        assert_eq!(
            batch.try_with_result(&job, |value| {
                let _profile = solarity_profiling::profile!("fixture.consumer.body");
                *value += 1;
                *value
            })?,
            Some(43),
        );
        let unwind = catch_unwind(AssertUnwindSafe(|| {
            let _ = batch.with_result(&job, |value| {
                *value += 1;
                panic!("fixture consumer unwinds after mutation");
            });
        }));
        assert!(unwind.is_err());
        assert_eq!(batch.with_result(&job, |value| *value)?, 44);
        let mut returned = Vec::new();
        batch.reclaim(&mut returned)?;
        assert_eq!(returned, [44]);
        origin.value("fixture.ready.end", mode, 0, 0);
    }
    drop(frame);
    cpu.shutdown()?;
    capture.shutdown()?;

    let contents = std::fs::read_to_string(path.with_extension("trace.csv"))?;
    let rows = contents.lines().skip(1).map(Row::parse).collect::<Vec<_>>();
    for (mode, class) in [("1", "frame"), ("2", "load")] {
        // Other unit tests may use the executor during this process-wide capture.
        // Fixture markers identify this exact phase and consuming thread.
        let marker = rows
            .iter()
            .find(|row| row.label() == "fixture.ready.begin" && row.owner() == mode)
            .ok_or("missing ready marker")?;
        let request = rows
            .iter()
            .find(|row| {
                row.id() == marker.parent() && row.label() == format!("cpu.{class}.request")
            })
            .ok_or("missing fixture request")?;
        let named = |label: &str| {
            rows.iter()
                .find(|row| row.label() == label && row.parent() == request.id())
                .ok_or_else(|| format!("missing trace {label}"))
        };
        let pending = interval(&rows, mode, "fixture.pending.begin", "fixture.pending.end")?;
        let ready = interval(&rows, mode, "fixture.ready.begin", "fixture.ready.end")?;
        let result_wait = format!("cpu.{class}.result_wait");
        let reclaim_wait = format!("cpu.{class}.reclaim_wait");
        let waits = rows.iter().filter(|row| {
            row.0[0] == marker.0[0] && (row.label() == result_wait || row.label() == reclaim_wait)
        });
        let mut saw_gate_wait = false;
        for row in waits {
            assert_eq!(row.parent(), request.id());
            let start = row.start()?;
            assert!(
                !pending.contains(&start),
                "nonblocking probe was charged as waiting"
            );
            assert!(
                !ready.contains(&start),
                "ready consumption was charged as waiting"
            );
            assert!(row.end()? <= ready.start);
            saw_gate_wait |=
                row.label() == result_wait && row.owner() == "1" && row.reason() == "1";
        }
        assert!(
            saw_gate_wait,
            "the real prerequisite wait lost its phase/node cause"
        );
        let consume_label = format!("cpu.{class}.consume");
        let consumes = rows
            .iter()
            .filter(|row| row.label() == consume_label && row.parent() == request.id())
            .collect::<Vec<_>>();
        assert_eq!(
            consumes.len(),
            3,
            "ordinary and unwound callbacks must be measured"
        );
        for consumed in &consumes {
            assert_eq!(consumed.parent(), request.id());
            assert_eq!(consumed.owner(), "1");
            assert!(ready.contains(&consumed.start()?));
            assert!(consumed.end()? <= ready.end);
            assert_eq!(consumed.0[4], request.0[4]);
            if class == "load" {
                assert_ne!(consumed.0[4], consumed.0[5]);
            }
        }
        assert!(rows.iter().any(|row| {
            row.label() == "fixture.consumer.body" && row.parent() == consumes[0].id()
        }));
        let reclaim = named(&format!("cpu.{class}.reclaim"))?;
        assert_eq!(reclaim.parent(), request.id());
        assert_eq!(reclaim.reason(), "1");
        assert!(ready.contains(&reclaim.start()?));
        for link in [
            "cpu.phase.wait_need",
            "cpu.phase.consume_need",
            "cpu.phase.reclaim_need",
        ] {
            assert!(
                rows.iter()
                    .any(|row| row.label() == link && row.0[3] == request.id())
            );
        }
    }
    std::fs::remove_dir_all(root)?;
    Ok(())
}

/// The fixture writes no quoted commas; access named trace columns explicitly.
struct Row<'a>(Vec<&'a str>);
impl<'a> Row<'a> {
    fn parse(line: &'a str) -> Self {
        Self(line.split(',').collect())
    }
    fn id(&self) -> &str {
        self.0[1]
    }
    fn parent(&self) -> &str {
        self.0[2]
    }
    fn label(&self) -> &str {
        self.0[9].trim_matches('"')
    }
    fn owner(&self) -> &str {
        self.0[10]
    }
    fn reason(&self) -> &str {
        self.0[11]
    }
    fn start(&self) -> Result<u64, std::num::ParseIntError> {
        self.0[6].parse()
    }
    fn end(&self) -> Result<u64, std::num::ParseIntError> {
        Ok(self.start()? + self.0[7].parse::<u64>()?)
    }
}

/// Ordered event boundaries assert inclusion, never elapsed-time performance.
fn interval(
    rows: &[Row<'_>],
    mode: &str,
    begin: &str,
    end: &str,
) -> Result<std::ops::Range<u64>, Box<dyn Error>> {
    let time = |label| -> Result<u64, Box<dyn Error>> {
        Ok(rows
            .iter()
            .find(|row| row.label() == label && row.owner() == mode)
            .ok_or("missing fixture boundary")?
            .start()?)
    };
    Ok(time(begin)?..time(end)?)
}
