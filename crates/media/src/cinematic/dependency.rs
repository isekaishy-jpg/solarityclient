//! FFmpeg demux, DivX decode, and RGBA conversion adapter.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use ffmpeg::format::Pixel;
use ffmpeg::media::Type;
use ffmpeg::software::scaling::{context::Context as ScalingContext, flag::Flags};
use ffmpeg::util::error::EAGAIN;
use ffmpeg::util::frame::video::Video;
use ffmpeg_next as ffmpeg;

use super::{CinematicError, CinematicVideoFrame};

/// Incremental decoder for one stock locale-loose AVI cinematic.
pub struct CinematicDecoder {
    path: PathBuf,
    input: ffmpeg::format::context::Input,
    video_stream_index: usize,
    video_time_base: ffmpeg::Rational,
    video: ffmpeg::decoder::Video,
    scaler: ScalingContext,
    eof_sent: bool,
}

impl CinematicDecoder {
    /// Opens the best video stream and prepares exact-size RGBA conversion.
    ///
    /// # Errors
    ///
    /// Returns [`CinematicError`] for FFmpeg initialization, container, stream,
    /// decoder, or scaler failures. No alternate codec or file is selected.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, CinematicError> {
        initialize_ffmpeg()?;
        let path = path.as_ref().to_path_buf();
        let input = ffmpeg::format::input(&path)
            .map_err(|source| CinematicError::adapter(&path, "open movie container", source))?;
        let stream = input
            .streams()
            .best(Type::Video)
            .ok_or_else(|| CinematicError::MissingVideo { path: path.clone() })?;
        let video_stream_index = stream.index();
        let video_time_base = stream.time_base();
        let context = ffmpeg::codec::context::Context::from_parameters(stream.parameters())
            .map_err(|source| CinematicError::adapter(&path, "read video parameters", source))?;
        let video = context
            .decoder()
            .video()
            .map_err(|source| CinematicError::adapter(&path, "open video decoder", source))?;
        let scaler = ScalingContext::get(
            video.format(),
            video.width(),
            video.height(),
            Pixel::RGBA,
            video.width(),
            video.height(),
            Flags::BILINEAR,
        )
        .map_err(|source| CinematicError::adapter(&path, "create RGBA scaler", source))?;
        Ok(Self {
            path,
            input,
            video_stream_index,
            video_time_base,
            video,
            scaler,
            eof_sent: false,
        })
    }

    /// Decodes the next timestamped tightly packed RGBA8 frame.
    ///
    /// # Errors
    ///
    /// Returns [`CinematicError`] when demux, decode, timestamp, conversion, or
    /// row-layout invariants fail. `None` means the decoder fully drained.
    pub fn next_video_frame(&mut self) -> Result<Option<CinematicVideoFrame>, CinematicError> {
        loop {
            let mut decoded = Video::empty();
            match self.video.receive_frame(&mut decoded) {
                Ok(()) => return self.convert_frame(&decoded).map(Some),
                Err(ffmpeg::Error::Other { errno }) if errno == EAGAIN => {}
                Err(ffmpeg::Error::Eof) => return Ok(None),
                Err(source) => {
                    return Err(CinematicError::adapter(
                        &self.path,
                        "receive decoded video frame",
                        source,
                    ));
                }
            }

            if self.eof_sent {
                return Ok(None);
            }
            let packet = self
                .input
                .packets()
                .find(|(stream, _packet)| stream.index() == self.video_stream_index)
                .map(|(_stream, packet)| packet);
            if let Some(packet) = packet {
                self.video.send_packet(&packet).map_err(|source| {
                    CinematicError::adapter(&self.path, "submit video packet", source)
                })?;
            } else {
                self.video.send_eof().map_err(|source| {
                    CinematicError::adapter(&self.path, "finish video stream", source)
                })?;
                self.eof_sent = true;
            }
        }
    }

    fn convert_frame(&mut self, decoded: &Video) -> Result<CinematicVideoFrame, CinematicError> {
        let timestamp = decoded
            .timestamp()
            .ok_or_else(|| CinematicError::MissingTimestamp {
                path: self.path.clone(),
            })?;
        let seconds = timestamp as f64 * f64::from(self.video_time_base);
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(CinematicError::InvalidTimestamp {
                path: self.path.clone(),
                timestamp,
            });
        }
        let mut rgba = Video::empty();
        self.scaler
            .run(decoded, &mut rgba)
            .map_err(|source| CinematicError::adapter(&self.path, "convert video frame", source))?;
        let width = rgba.width();
        let height = rgba.height();
        let row_bytes = usize::try_from(width)
            .ok()
            .and_then(|width| width.checked_mul(4))
            .ok_or_else(|| CinematicError::FrameSize {
                path: self.path.clone(),
                width,
                height,
            })?;
        let byte_count =
            row_bytes
                .checked_mul(height as usize)
                .ok_or_else(|| CinematicError::FrameSize {
                    path: self.path.clone(),
                    width,
                    height,
                })?;
        if rgba.stride(0) < row_bytes {
            return Err(CinematicError::FrameStride {
                path: self.path.clone(),
                stride: rgba.stride(0),
                row_bytes,
            });
        }
        let mut pixels = Vec::with_capacity(byte_count);
        for row in 0..height as usize {
            let start = row * rgba.stride(0);
            let end = start + row_bytes;
            pixels.extend_from_slice(&rgba.data(0)[start..end]);
        }
        CinematicVideoFrame::new(width, height, Duration::from_secs_f64(seconds), pixels).map_err(
            |()| CinematicError::FrameSize {
                path: self.path.clone(),
                width,
                height,
            },
        )
    }
}

fn initialize_ffmpeg() -> Result<(), CinematicError> {
    static INITIALIZED: OnceLock<Result<(), String>> = OnceLock::new();
    match INITIALIZED.get_or_init(|| ffmpeg::init().map_err(|source| source.to_string())) {
        Ok(()) => Ok(()),
        Err(message) => Err(CinematicError::Initialization {
            message: message.clone(),
        }),
    }
}
