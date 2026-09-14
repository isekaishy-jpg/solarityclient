//! One-second snapshots swap buffers briefly, then format and write off-thread.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::recorder::{Aggregate, EVENT_CAPACITY, Metric, ROW_CAPACITY, Sample, Shard, registry};

/// Retained writer-side scratch and totals are bounded by registered thread count.
struct ThreadRows {
    shard: Arc<Shard>,
    scratch: Vec<Aggregate>,
    totals: Vec<Aggregate>,
    events: Vec<Sample>,
    dropped_at_start: u64,
    dropped_events_at_start: u64,
}

/// Writes parseable intervals plus a cumulative summary and capture health metadata.
pub(super) fn run(
    epoch: u64,
    file: File,
    path: &Path,
    identity: &str,
    stop: Receiver<()>,
) -> io::Result<()> {
    let started = Instant::now();
    let overflow_start = crate::recorder::overflows();
    let mut output = BufWriter::new(file);
    let mut events = BufWriter::new(File::create(path.with_extension("events.csv"))?);
    writeln!(
        events,
        "elapsed_seconds,completion_frame,thread,scope,phase,frame_lane,unit,value"
    )?;
    header(&mut output)?;
    let mut threads: Vec<ThreadRows> = Vec::new();
    let mut metrics = Vec::new();
    let mut resources = BufWriter::new(File::create(path.with_extension("resources.csv"))?);
    writeln!(
        resources,
        "elapsed_seconds,kind,pid,os_thread_id,kernel_100ns,user_100ns,cycles,available,working_set_bytes,private_bytes"
    )?;
    let mut snapshots = 0_u64;
    let mut snapshot_ns = 0_u128;
    loop {
        let finished = match stop.recv_timeout(Duration::from_secs(1)) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => true,
            Err(RecvTimeoutError::Timeout) => false,
        };
        let snapshot_started = Instant::now();
        let new_shards = {
            let registry = registry()
                .lock()
                .map_err(|_| io::Error::other("profile registry unavailable"))?;
            metrics.clone_from(&registry.metrics);
            registry.shards[threads.len()..].to_vec()
        };
        // Large scratch allocations never hold the registration lock.
        for shard in new_shards {
            threads.push(ThreadRows {
                dropped_at_start: shard.dropped.load(Ordering::Relaxed),
                dropped_events_at_start: shard.dropped_events.load(Ordering::Relaxed),
                shard,
                scratch: vec![Aggregate::default(); ROW_CAPACITY],
                totals: vec![Aggregate::default(); ROW_CAPACITY],
                events: Vec::with_capacity(EVENT_CAPACITY),
            });
        }
        let seconds = started.elapsed().as_secs_f64();
        for thread in &mut threads {
            {
                let mut data = thread
                    .shard
                    .data
                    .lock()
                    .map_err(|_| io::Error::other("profile thread buffer unavailable"))?;
                if data.epoch != epoch {
                    continue;
                }
                thread.dropped_at_start = data.dropped_start;
                thread.dropped_events_at_start = data.dropped_events_start;
                std::mem::swap(&mut data.rows, &mut thread.scratch);
                std::mem::swap(&mut data.events, &mut thread.events);
            }
            // A producer may register a phase after the registry snapshot but
            // before this buffer swap. Its definition exists before its sample.
            if thread.scratch[metrics.len() * 2..]
                .iter()
                .any(|row| row.count != 0)
            {
                let registry = registry()
                    .lock()
                    .map_err(|_| io::Error::other("profile registry unavailable"))?;
                metrics.clone_from(&registry.metrics);
            }
            for (index, row) in thread.scratch.iter_mut().enumerate() {
                if row.count == 0 {
                    continue;
                }
                if let Some(metric) = metrics.get(index / 2) {
                    write_row(
                        &mut output,
                        seconds,
                        &thread.shard.name,
                        metric,
                        index % 2 != 0,
                        row,
                    )?;
                    thread.totals[index].merge(row);
                }
                *row = Aggregate::default();
            }
            for sample in thread.events.drain(..) {
                if let Some(metric) = metrics.get(sample.metric) {
                    writeln!(
                        events,
                        "{seconds:.6},{},{},{},{},{},{},{}",
                        sample.frame,
                        quote(&thread.shard.name),
                        quote(metric.scope),
                        quote(metric.phase),
                        if sample.detailed {
                            "detail"
                        } else {
                            "ordinary"
                        },
                        metric.unit,
                        sample.value
                    )?;
                }
            }
        }
        if snapshots.is_multiple_of(5) || finished {
            crate::host::write_snapshot(&mut resources, seconds)?;
            resources.flush()?;
        }
        writeln!(
            resources,
            "{seconds:.6},registered_metrics,0,0,0,0,{},1,0,0",
            metrics.len()
        )?;
        output.flush()?;
        events.flush()?;
        resources.flush()?;
        snapshot_ns += snapshot_started.elapsed().as_nanos();
        snapshots += 1;
        if finished {
            break;
        }
    }
    let mut summary = BufWriter::new(File::create(path.with_extension("summary.csv"))?);
    header(&mut summary)?;
    for thread in &threads {
        for (index, row) in thread
            .totals
            .iter()
            .enumerate()
            .filter(|(_, row)| row.count != 0)
        {
            if let Some(metric) = metrics.get(index / 2) {
                write_row(
                    &mut summary,
                    started.elapsed().as_secs_f64(),
                    &thread.shard.name,
                    metric,
                    index % 2 != 0,
                    row,
                )?;
            }
        }
    }
    summary.flush()?;
    let dropped: u64 = threads
        .iter()
        .map(|thread| {
            thread
                .shard
                .dropped
                .load(Ordering::Relaxed)
                .saturating_sub(thread.dropped_at_start)
        })
        .sum();
    let mut metadata = BufWriter::new(File::create(path.with_extension("txt"))?);
    let dropped_events: u64 = threads
        .iter()
        .map(|thread| {
            thread
                .shard
                .dropped_events
                .load(Ordering::Relaxed)
                .saturating_sub(thread.dropped_events_at_start)
        })
        .sum();
    writeln!(metadata, "dropped_event_rows={dropped_events}")?;
    writeln!(
        metadata,
        "{identity}\ncapture_generation={epoch}\nduration_seconds={}\ndetail_frame_interval={}\nregistered_metrics={}\nregistered_threads={}\ndropped_samples={dropped}\ncapacity_overflows={}\nwriter_snapshots={snapshots}\nwriter_wall_time_ns={snapshot_ns}",
        started.elapsed().as_secs_f64(),
        crate::scope::DETAIL_INTERVAL,
        metrics.len(),
        threads.len(),
        crate::recorder::overflows().saturating_sub(overflow_start)
    )?;
    writeln!(
        metadata,
        "\nDurations use nanoseconds. Totals include observer bookkeeping; phase marks exclude recorder bookkeeping between phases.\nScope totals overlap child scopes and worker/GPU activity: do not add them.\nDetail-frame rows are separate from ordinary captured frames. First-use registration and buffer allocation are cold capture costs.\nPercentiles are histogram upper bounds; max is exact. Value metrics use mean/max, not time percentiles.\nThe writer never runs on a measured thread. Busy snapshot locks drop samples instead of waiting.\nInstrumentation has nonzero overhead. Use the optimized overhead benchmark and compare ordinary/detail frame rows; no automatic overhead subtraction is applied."
    )?;
    metadata.flush()
}

/// CSV preserves scope/phase relationships and the detail-frame classification.
fn header(output: &mut impl Write) -> io::Result<()> {
    writeln!(
        output,
        "elapsed_seconds,thread,scope,phase,unit,probe_sampling,frame_lane,count,total,mean,p50_upper,p95_upper,p99_upper,max"
    )
}

/// Quotes only diagnostic labels, never gameplay strings or network payloads.
fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

/// Converts fixed aggregates outside every recorder lock.
fn write_row(
    output: &mut impl Write,
    seconds: f64,
    thread: &str,
    metric: &Metric,
    lane: bool,
    row: &Aggregate,
) -> io::Result<()> {
    writeln!(
        output,
        "{seconds:.6},{},{},{},{},{},{},{},{},{:.3},{},{},{},{}",
        quote(thread),
        quote(metric.scope),
        quote(metric.phase),
        metric.unit,
        if metric.detailed { "detail" } else { "coarse" },
        if lane { "detail" } else { "ordinary" },
        row.count,
        row.sum,
        row.sum as f64 / row.count.max(1) as f64,
        row.percentile(50),
        row.percentile(95),
        row.percentile(99),
        row.maximum
    )
}
