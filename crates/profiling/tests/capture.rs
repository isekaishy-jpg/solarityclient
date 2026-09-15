//! Real writer lifecycle, async poll semantics and generation isolation.

use std::future::Future;
use std::io;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use solarity_profiling::{Capture, Site, begin_frame, enabled, generation};

/// Exercises one process-wide capture owner, avoiding parallel global test state.
#[test]
fn records_bounded_scopes_and_restarts_without_stale_samples() -> io::Result<()> {
    let root = std::env::temp_dir().join(format!("solarity-profile-test-{}", std::process::id()));
    let mut capture = Capture::new(&root, "fixture=true".to_owned());
    assert!(!enabled());
    let mut evaluated = false;
    solarity_profiling::profile_value!("disabled", {
        evaluated = true;
        1
    });
    assert!(!evaluated);
    solarity_profiling::profile_event_value!("disabled.event", {
        evaluated = true;
        1
    });
    assert!(!evaluated);
    let (active, first) = capture.toggle()?;
    assert!(active);
    let epoch = generation();
    let stale = solarity_profiling::profile!("stale.scope");
    let stale_cycles = solarity_profiling::profile_cycles!("stale.cycles");
    for _ in 0..130 {
        let _frame = begin_frame();
        let _detail = solarity_profiling::detail_profile!("fixture.detail");
        solarity_profiling::profile_value!("fixture.value", 7);
        solarity_profiling::profile_event_value!("fixture.change", 3);
        drop(solarity_profiling::profile_cycles!("fixture.charged"));
    }
    std::thread::Builder::new()
        .name("fixture-worker".to_owned())
        .spawn(|| {
            let mut scope = solarity_profiling::profile!("fixture.worker");
            scope.mark("second");
            scope.mark("first");
        })?
        .join()
        .map_err(|_| io::Error::other("fixture worker panicked"))?;

    let mut future = std::pin::pin!(async {
        let mut pending = true;
        solarity_profiling::profile_await!(
            "fixture.async",
            std::future::poll_fn(|_| {
                if std::mem::take(&mut pending) {
                    Poll::Pending
                } else {
                    Poll::Ready(19)
                }
            })
        )
    });
    let mut context = Context::from_waker(Waker::noop());
    assert!(future.as_mut().poll(&mut context).is_pending());
    assert_eq!(future.as_mut().poll(&mut context), Poll::Ready(19));
    assert!(!capture.toggle()?.0);
    capture.shutdown()?;
    let summary = std::fs::read_to_string(first.with_extension("summary.csv"))?;
    assert!(summary.contains("fixture-worker"));
    assert!(summary.contains("\"fixture.async\",\"\",ns,coarse,ordinary,2,"));
    assert!(summary.contains("\"fixture.detail\",\"\",ns,detail,detail,2,"));
    let events = std::fs::read_to_string(first.with_extension("events.csv"))?;
    assert_eq!(
        events
            .lines()
            .filter(|row| row.contains("\"frame.live\",\"\""))
            .count(),
        130
    );
    assert!(events.contains("\"fixture.value\",\"\",detail,value,7"));
    assert!(events.contains("\"fixture.change\",\"\",ordinary,value,3"));
    assert!(summary.contains("fixture.charged.cycles_available"));
    let (_, second) = capture.toggle()?;
    assert_ne!(epoch, generation());
    static GPU: Site = Site::new("stale.gpu", true);
    GPU.duration(epoch, "", Duration::from_millis(100));
    drop(stale);
    drop(stale_cycles);
    static OLD_VALUE: Site = Site::counter("stale.value");
    OLD_VALUE.event_value(epoch, 1);
    OLD_VALUE.value_for_generation(epoch, 1);
    drop(solarity_profiling::profile!("fresh.scope"));
    capture.shutdown()?;
    let summary = std::fs::read_to_string(second.with_extension("summary.csv"))?;
    assert!(!summary.contains("stale."));
    assert!(summary.contains("fresh.scope"));
    assert!(!enabled());
    exercise_causal_provenance(&root)?;
    std::fs::remove_dir_all(root)
}

/// Cross-frame worker, GPU and consumer edges preserve the requesting sample.
fn exercise_causal_provenance(root: &std::path::Path) -> io::Result<()> {
    let mut capture = Capture::new(root, "chain=true".to_owned());
    let (_, first) = capture.toggle()?;
    while !solarity_profiling::detail_enabled() {
        drop(begin_frame());
    }
    let owner = solarity_profiling::profile!("chain.owner");
    let request = solarity_profiling::TraceContext::capture().fork("chain.request");
    request.asset(7, "MODEL\\FIXTURE.M2");
    drop(owner);
    for _ in 0..3 {
        drop(begin_frame());
    }
    assert!(!solarity_profiling::detail_enabled());
    std::thread::spawn(move || {
        let _context = request.enter();
        let mut work = solarity_profiling::profile!("chain.worker");
        work.trace_owner(77, 3);
        work.mark("prepared");
        static PHASE: Site = Site::new("chain.phases", false);
        PHASE.cpu_duration(generation(), "recorded wait", Duration::from_millis(1));
        solarity_profiling::profile_value!("chain.output", 9);
    })
    .join()
    .map_err(|_| io::Error::other("chain worker panicked"))?;
    request.link("chain.consume");
    request.gpu_duration("fixture GPU", Duration::from_millis(2));
    capture.shutdown()?;
    let summary = std::fs::read_to_string(first.with_extension("summary.csv"))?;
    assert!(summary.contains("\"chain.output\",\"\",value,coarse,detail,1,"));
    let trace = std::fs::read_to_string(first.with_extension("trace.csv"))?;
    let rows: Vec<Vec<&str>> = trace
        .lines()
        .skip(1)
        .map(|row| row.split(',').collect())
        .collect();
    let named = |name: &str| {
        rows.iter()
            .find(|row| row[9].trim_matches('"') == name)
            .ok_or_else(|| io::Error::other(format!("missing fixture trace operation {name}")))
    };
    let owner = named("chain.owner")?;
    let request_row = named("chain.request")?;
    let worker = named("chain.worker")?;
    assert_eq!(request_row[2], owner[1]);
    assert_eq!(worker[2], request_row[1]);
    assert_eq!(worker[4], request_row[4]);
    assert_ne!(worker[4], worker[5]);
    assert_eq!(worker[10], "77");
    assert_eq!(worker[11], "3");
    let timing = named("recorded wait")?;
    assert_eq!(timing[2], worker[1]);
    assert_eq!(timing[7], "1000000");
    assert_eq!(timing[8], "timing");
    assert_eq!(named("chain.consume")?[3], request_row[1]);
    assert_eq!(named("fixture GPU")?[2], request_row[1]);
    assert!(trace.contains("MODEL\\FIXTURE.M2"));
    let (_, second) = capture.toggle()?;
    {
        let _old = request.enter();
        drop(solarity_profiling::profile!("chain.stale"));
    }
    request.link("chain.stale.consume");
    capture.shutdown()?;
    assert!(!std::fs::read_to_string(second.with_extension("trace.csv"))?.contains("chain.stale"));
    Ok(())
}
