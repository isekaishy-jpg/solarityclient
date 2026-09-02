//! FFmpeg demux, DivX decode, and RGBA conversion adapter.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use ffmpeg::ChannelLayout;
use ffmpeg::format::{Pixel, Sample, sample::Type as SampleType};
use ffmpeg::media::Type;
use ffmpeg::software::resampling::Context as ResamplingContext;
use ffmpeg::software::scaling::{context::Context as ScalingContext, flag::Flags};
use ffmpeg::util::error::EAGAIN;
use ffmpeg::util::frame::audio::Audio;
use ffmpeg::util::frame::video::Video;
use ffmpeg_next as ffmpeg;

use super::{CinematicAudioFrame, CinematicError, CinematicVideoFrame};

const CINEMATIC_SAMPLE_RATE_HZ: u32 = 44_100;

struct AudioDecoder {
    stream_index: usize,
    decoder: ffmpeg::decoder::Audio,
    resampler: ResamplingContext,
    drained: bool,
}

/// Incremental decoder for one stock locale-loose AVI cinematic.
pub struct CinematicDecoder {
    path: PathBuf,
    input: ffmpeg::format::context::Input,
    video_stream_index: usize,
    video_time_base: ffmpeg::Rational,
    video: ffmpeg::decoder::Video,
    scaler: ScalingContext,
    audio: Option<AudioDecoder>,
    audio_frames: Vec<CinematicAudioFrame>,
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
        let audio = input
            .streams()
            .best(Type::Audio)
            .map(|stream| {
                let stream_index = stream.index();
                let context = ffmpeg::codec::context::Context::from_parameters(stream.parameters())
                    .map_err(|source| {
                        CinematicError::adapter(&path, "read audio parameters", source)
                    })?;
                let decoder = context.decoder().audio().map_err(|source| {
                    CinematicError::adapter(&path, "open audio decoder", source)
                })?;
                let source_layout = if decoder.channel_layout().channels() == 0 {
                    ChannelLayout::default(i32::from(decoder.channels()))
                } else {
                    decoder.channel_layout()
                };
                let resampler = ResamplingContext::get(
                    decoder.format(),
                    source_layout,
                    decoder.rate(),
                    Sample::I16(SampleType::Packed),
                    ChannelLayout::STEREO,
                    CINEMATIC_SAMPLE_RATE_HZ,
                )
                .map_err(|source| {
                    CinematicError::adapter(&path, "create cinematic audio resampler", source)
                })?;
                Ok(AudioDecoder {
                    stream_index,
                    decoder,
                    resampler,
                    drained: false,
                })
            })
            .transpose()?;
        Ok(Self {
            path,
            input,
            video_stream_index,
            video_time_base,
            video,
            scaler,
            audio,
            audio_frames: Vec::new(),
            eof_sent: false,
        })
    }

    /// Takes audio blocks decoded while advancing the interleaved AVI stream.
    ///
    /// Each block contains interleaved stereo signed-16 samples at 44.1 kHz,
    /// matching build 12340's requested output format.
    pub fn take_audio_frames(&mut self) -> Vec<CinematicAudioFrame> {
        std::mem::take(&mut self.audio_frames)
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
            let audio_stream_index = self.audio.as_ref().map(|audio| audio.stream_index);
            let packet = self.input.packets().find(|(stream, _packet)| {
                stream.index() == self.video_stream_index
                    || audio_stream_index == Some(stream.index())
            });
            if let Some((stream, packet)) = packet {
                if stream.index() == self.video_stream_index {
                    self.video.send_packet(&packet).map_err(|source| {
                        CinematicError::adapter(&self.path, "submit video packet", source)
                    })?;
                } else if let Some(audio) = self.audio.as_mut() {
                    audio.decoder.send_packet(&packet).map_err(|source| {
                        CinematicError::adapter(&self.path, "submit audio packet", source)
                    })?;
                    self.receive_audio_frames()?;
                }
            } else {
                self.video.send_eof().map_err(|source| {
                    CinematicError::adapter(&self.path, "finish video stream", source)
                })?;
                if let Some(audio) = self.audio.as_mut() {
                    audio.decoder.send_eof().map_err(|source| {
                        CinematicError::adapter(&self.path, "finish audio stream", source)
                    })?;
                    self.receive_audio_frames()?;
                }
                self.eof_sent = true;
            }
        }
    }

    fn receive_audio_frames(&mut self) -> Result<(), CinematicError> {
        let Some(audio) = self.audio.as_mut() else {
            return Ok(());
        };
        loop {
            let mut decoded = Audio::empty();
            match audio.decoder.receive_frame(&mut decoded) {
                Ok(()) => {
                    let mut converted = Audio::empty();
                    audio
                        .resampler
                        .run(&decoded, &mut converted)
                        .map_err(|source| {
                            CinematicError::adapter(
                                &self.path,
                                "convert cinematic audio frame",
                                source,
                            )
                        })?;
                    let samples = converted
                        .plane::<(i16, i16)>(0)
                        .iter()
                        .flat_map(|&(left, right)| [left, right])
                        .collect();
                    self.audio_frames.push(CinematicAudioFrame::new(samples));
                }
                Err(ffmpeg::Error::Other { errno }) if errno == EAGAIN => return Ok(()),
                Err(ffmpeg::Error::Eof) if !audio.drained => {
                    audio.drained = true;
                    while let Some(delay) = audio.resampler.delay() {
                        let sample_capacity = usize::try_from(delay.output)
                            .ok()
                            .and_then(|samples| samples.checked_add(1))
                            .ok_or_else(|| CinematicError::Initialization {
                                message: "cinematic audio drain size overflowed".to_owned(),
                            })?;
                        let output = *audio.resampler.output();
                        // `swr_convert_frame` requires the drain frame to carry
                        // the configured output format, layout, and capacity.
                        // Passing `Audio::empty()` reports AVERROR_OUTPUT_CHANGED
                        // at the natural end of the stock Wrath AVI.
                        let mut converted =
                            Audio::new(output.format, sample_capacity, output.channel_layout);
                        let remaining =
                            audio.resampler.flush(&mut converted).map_err(|source| {
                                CinematicError::adapter(
                                    &self.path,
                                    "drain cinematic audio resampler",
                                    source,
                                )
                            })?;
                        let samples = converted
                            .plane::<(i16, i16)>(0)
                            .iter()
                            .flat_map(|&(left, right)| [left, right])
                            .collect();
                        self.audio_frames.push(CinematicAudioFrame::new(samples));
                        if remaining.is_none() {
                            break;
                        }
                        if remaining.is_some_and(|remaining| remaining.output >= delay.output) {
                            // libswresample may retain a fractional delay that
                            // cannot form another output sample. It reports the
                            // same rounded count indefinitely; stock playback
                            // has no sample to queue at that point.
                            break;
                        }
                    }
                    return Ok(());
                }
                Err(ffmpeg::Error::Eof) => return Ok(()),
                Err(source) => {
                    return Err(CinematicError::adapter(
                        &self.path,
                        "receive decoded audio frame",
                        source,
                    ));
                }
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
