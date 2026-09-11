//! User-requested framebuffer capture and worker-owned screenshot encoding.

use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use solarity_cpu::{CpuError, CpuExecutor, CpuTask};
use solarity_rendering::{CapturedFrame, VulkanRenderer};

/// Settings and originating UI domain retained until completion.
#[derive(Clone, Copy)]
pub(super) struct ScreenshotRequest {
    world: bool,
    tga: bool,
    quality: u8,
}

impl ScreenshotRequest {
    pub(super) fn new(world: bool, format: &str, quality: &str) -> Self {
        // 4A84A0 accepts an unsigned integer, clamps 1..10, then truncates
        // 45 + 5.5 * quality. Default 3 therefore encodes at 61, not 30.
        // SStrToInt (76F0D0) accepts a leading minus and a decimal prefix,
        // without skipping whitespace or a plus, and wraps at 32 bits.
        let negative = quality.starts_with('-');
        let digits = quality
            .as_bytes()
            .get(usize::from(negative)..)
            .unwrap_or_default();
        let quality = digits
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .fold(0_u32, |value, byte| {
                value.wrapping_mul(10).wrapping_add(u32::from(byte - b'0'))
            });
        let quality = if negative {
            quality.wrapping_neg()
        } else {
            quality
        };
        Self {
            world,
            // 4A8570 selects TGA only for this case-insensitive token.
            tga: format.eq_ignore_ascii_case("tga"),
            quality: (45.0 + 5.5 * quality.clamp(1, 10) as f32) as u8,
        }
    }
}

pub(super) struct ScreenshotCompletion {
    pub(super) world: bool,
    pub(super) result: Result<PathBuf, String>,
}

/// One pending native request coalesces repeated keys before presentation.
pub(super) struct RuntimeScreenshots {
    directory: PathBuf,
    pending: Option<ScreenshotRequest>,
    capturing: Option<ScreenshotRequest>,
    writing: Option<(bool, CpuTask<Result<PathBuf, String>>)>,
}

impl RuntimeScreenshots {
    pub(super) fn new(profile_root: &Path) -> Self {
        Self {
            directory: profile_root.join("Screenshots"),
            pending: None,
            capturing: None,
            writing: None,
        }
    }

    pub(super) fn request(&mut self, request: ScreenshotRequest) {
        self.pending = Some(request);
    }

    /// Ordinary frames allocate nothing and never wait for GPU/encoder work.
    pub(super) fn poll(
        &mut self,
        renderer: &mut VulkanRenderer,
        cpu: &CpuExecutor,
    ) -> Option<ScreenshotCompletion> {
        if self
            .writing
            .as_ref()
            .is_some_and(|(_, task)| task.is_finished())
        {
            let (world, task) = self.writing.take()?;
            return Some(ScreenshotCompletion {
                world,
                result: task.join().map_err(|e| e.to_string()).and_then(|r| r),
            });
        }
        if let Some(request) = self.capturing {
            // Keep the completed GPU capture owned by the renderer until an
            // encoder slot is actually reserved. Full queues are temporary;
            // consuming the frame into a rejected closure would lose it.
            let permit = match cpu.try_reserve() {
                Ok(permit) => permit,
                Err(CpuError::AtCapacity { .. }) => return None,
                Err(error) => {
                    self.capturing = None;
                    return Some(ScreenshotCompletion {
                        world: request.world,
                        result: Err(error.to_string()),
                    });
                }
            };
            let result = match renderer.take_captured_frame() {
                Ok(None) => return None,
                Ok(Some(frame)) => {
                    let directory = self.directory.clone();
                    Ok(permit.submit(move || write_screenshot(&directory, &frame, request)))
                }
                Err(error) => Err(error.to_string()),
            };
            self.capturing = None;
            match result {
                Ok(task) => self.writing = Some((request.world, task)),
                Err(error) => {
                    return Some(ScreenshotCompletion {
                        world: request.world,
                        result: Err(error),
                    });
                }
            }
        }
        if self.capturing.is_none()
            && self.writing.is_none()
            && let Some(request) = self.pending.take()
        {
            match renderer.request_frame_capture() {
                Ok(()) => self.capturing = Some(request),
                Err(error) => {
                    return Some(ScreenshotCompletion {
                        world: request.world,
                        result: Err(error.to_string()),
                    });
                }
            }
        }
        None
    }
}

fn write_screenshot(
    directory: &Path,
    frame: &CapturedFrame,
    request: ScreenshotRequest,
) -> Result<PathBuf, String> {
    let timestamp = crate::platform::screenshot_timestamp()
        .ok_or_else(|| "could not read the local screenshot date".to_owned())?;
    write_dated_screenshot(directory, frame, request, &timestamp).map_err(|e| e.to_string())
}

fn write_dated_screenshot(
    directory: &Path,
    frame: &CapturedFrame,
    request: ScreenshotRequest,
    timestamp: &str,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    fs::create_dir_all(directory)?;
    let extension = if request.tga { "tga" } else { "jpg" };
    // Preserve multiple captures in one second, without replacing an existing file.
    let mut ordinal = 0_u32;
    let (path, file) = loop {
        let suffix = if ordinal == 0 {
            String::new()
        } else {
            format!("_{ordinal}")
        };
        let path = directory.join(format!("WoWScrnShot_{timestamp}{suffix}.{extension}"));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => break (path, file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => ordinal += 1,
            Err(e) => return Err(e.into()),
        }
    };
    let mut writer = BufWriter::new(file);
    let (width, height) = frame.extent();
    let result = (|| -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if request.tga {
            image::codecs::tga::TgaEncoder::new(&mut writer).encode(
                frame.rgba8(),
                width,
                height,
                image::ExtendedColorType::Rgba8,
            )?;
        } else {
            let rgb = frame
                .rgba8()
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|pixel| pixel[..3].iter().copied())
                .collect::<Vec<_>>();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, request.quality)
                .encode(&rgb, width, height, image::ExtendedColorType::Rgb8)?;
        }
        writer.flush()?;
        Ok(())
    })();
    drop(writer);
    if result.is_err() {
        let _ = fs::remove_file(&path);
    }
    result.map(|()| path)
}

#[cfg(test)]
#[path = "../../tests/application/screenshot.rs"]
mod tests;
