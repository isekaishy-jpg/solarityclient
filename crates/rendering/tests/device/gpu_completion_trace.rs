//! The driver wait retains its requesting frame while native service stays separate.

use super::{GpuCompletionService, HostOutput};
use crate::device::VulkanError;
use ash::vk::{self, Handle};
use solarity_cpu::CoordinatorNotifier;
use solarity_profiling::{Capture, begin_frame};
use std::{
    error::Error,
    sync::{Arc, mpsc},
};

/// A notification acknowledges publication after the host has returned its fence.
struct Signal(mpsc::Sender<()>);
impl CoordinatorNotifier for Signal {
    fn notify(&self) {
        let _sent = self.0.send(());
    }
}

/// Channels control backend return and native observation; no GPU or sleep is needed.
#[test]
fn gpu_host_wait_links_to_its_request_and_ready_consumption_does_not_drain()
-> Result<(), Box<dyn Error>> {
    let root = std::env::temp_dir().join(format!("solarity-gpu-wait-trace-{}", std::process::id()));
    let mut capture = Capture::new(&root, "fixture=gpu-wait-trace".to_owned());
    let (_, path) = capture.toggle()?;
    let frame = begin_frame();
    let (started, start) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let (published, publication) = mpsc::channel();
    let mut service = GpuCompletionService::new(
        move |_| {
            started
                .send(())
                .unwrap_or_else(|_| panic!("fixture observes host start"));
            released
                .recv()
                .unwrap_or_else(|_| panic!("fixture releases host wait"));
            Ok(HostOutput::Complete)
        },
        Arc::new(Signal(published)),
    )?;
    let pending = solarity_profiling::profile!("fixture.gpu_pending");
    service.wait_for::<VulkanError>(vk::Fence::from_raw(7), |completion| {
        start.recv().unwrap_or_else(|_| panic!("host started"));
        assert!(!completion.is_ready());
        {
            let _native = solarity_profiling::profile!("fixture.native_service");
            release
                .send(())
                .unwrap_or_else(|_| panic!("host release sent"));
        }
        publication
            .recv()
            .unwrap_or_else(|_| panic!("host published"));
        assert!(completion.is_ready());
        Ok(())
    })?;
    drop(pending);
    // Ready readers cannot create a host request or invoke native service.
    service.wait_for_pending::<VulkanError>(
        [vk::Fence::from_raw(8)],
        |_| Ok(true),
        |_| panic!("ready source must not wait"),
    )?;
    drop(service);
    drop(frame);
    capture.shutdown()?;

    let trace = std::fs::read_to_string(path.with_extension("trace.csv"))?;
    let rows = trace
        .lines()
        .skip(1)
        .map(|line| line.split(',').collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let named = |label: &str| {
        rows.iter()
            .find(|row| row[9].trim_matches('"') == label)
            .ok_or_else(|| format!("missing trace {label}"))
    };
    let pending = named("fixture.gpu_pending")?;
    // Other completion fixtures can run during the process-wide capture. Follow
    // this test's unique parent instead of counting unrelated host requests.
    let request = rows
        .iter()
        .find(|row| {
            row[9].trim_matches('"') == "rendering.gpu_completion.request" && row[2] == pending[1]
        })
        .ok_or("missing fixture host request")?;
    let child = |label: &str| {
        rows.iter()
            .find(|row| row[9].trim_matches('"') == label && row[2] == request[1])
            .ok_or_else(|| format!("missing host child {label}"))
    };
    let host = child("rendering.gpu_completion.host_wait")?;
    let returned = child("rendering.gpu_completion.host_return")?;
    let native = named("fixture.native_service")?;
    assert_eq!(request[2], pending[1]);
    assert_eq!(host[2], request[1]);
    assert_eq!(host[4], request[4]);
    assert_eq!(returned[2], request[1]);
    assert_eq!(native[2], pending[1]);
    assert_ne!(host[0], native[0]);
    assert!(host[0].contains("solarity-gpu-completion"));
    let host_end = host[6].parse::<u64>()? + host[7].parse::<u64>()?;
    assert!(host_end <= returned[6].parse::<u64>()?);
    let observations = rows.iter().filter(|row| {
        row[9].trim_matches('"') == "rendering.gpu_completion.observe" && row[3] == request[1]
    });
    let mut observed = 0;
    for observation in observations {
        assert_eq!(observation[3], request[1]);
        assert!(returned[6].parse::<u64>()? <= observation[6].parse::<u64>()?);
        observed += 1;
    }
    assert_eq!(
        observed, 2,
        "finish and release each observe the retained result"
    );
    assert_eq!(
        rows.iter()
            .filter(
                |row| row[9].trim_matches('"') == "rendering.gpu_completion.request"
                    && row[2] == pending[1]
            )
            .count(),
        1
    );
    assert!(!rows.iter().any(|row| row[9].trim_matches('"')
        == "rendering.gpu_completion.drain_wait"
        && row[2] == request[1]));
    std::fs::remove_dir_all(root)?;
    Ok(())
}
