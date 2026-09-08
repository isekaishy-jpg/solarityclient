//! FFmpeg demux, DivX decode, and RGBA conversion adapter.

use std::collections::VecDeque;
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

use super::bitstream::PackedFrameFilter;
use super::{CinematicAudioFrame, CinematicError, CinematicVideoFrame};

const CINEMATIC_SAMPLE_RATE_HZ: u32 = 44_100;

/// Constant-rate stock AVI clock used when packed B-frame drain output loses PTS.
struct CinematicFrameClock {
    time_base: ffmpeg::Rational,
    frame_interval: Duration,
    last_presentation_time: Option<Duration>,
}

impl CinematicFrameClock {
    fn new(
        path: &Path,
        time_base: ffmpeg::Rational,
        frame_rate: ffmpeg::Rational,
    ) -> Result<Self, CinematicError> {
        let numerator = frame_rate.numerator();
        let denominator = frame_rate.denominator();
        if numerator <= 0 || denominator <= 0 {
            return Err(CinematicError::InvalidFrameRate {
                path: path.to_path_buf(),
                numerator,
                denominator,
            });
        }
        let seconds = f64::from(denominator) / f64::from(numerator);
        if !seconds.is_finite() || seconds <= 0.0 {
            return Err(CinematicError::InvalidFrameRate {
                path: path.to_path_buf(),
                numerator,
                denominator,
            });
        }
        Ok(Self {
            time_base,
            frame_interval: Duration::from_secs_f64(seconds),
            last_presentation_time: None,
        })
    }

    fn resolve(&mut self, path: &Path, timestamp: Option<i64>) -> Result<Duration, CinematicError> {
        let presentation_time = if let Some(timestamp) = timestamp {
            let seconds = timestamp as f64 * f64::from(self.time_base);
            if !seconds.is_finite() || seconds < 0.0 {
                return Err(CinematicError::InvalidTimestamp {
                    path: path.to_path_buf(),
                    timestamp,
                });
            }
            Duration::from_secs_f64(seconds)
        } else {
            // The build-12340 locale AVI is constant-rate (`strh` scale/rate)
            // and its packed DivX B-frame tail can drain without FFmpeg PTS.
            // Stock advances that stream by its authored frame cadence.
            self.last_presentation_time
                .and_then(|time| time.checked_add(self.frame_interval))
                .ok_or_else(|| CinematicError::MissingTimestamp {
                    path: path.to_path_buf(),
                })?
        };
        if let Some(previous) = self.last_presentation_time
            && presentation_time < previous
        {
            return Err(CinematicError::NonMonotonicTimestamp {
                path: path.to_path_buf(),
                previous,
                current: presentation_time,
            });
        }
        self.last_presentation_time = Some(presentation_time);
        Ok(presentation_time)
    }
}

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
    video_clock: CinematicFrameClock,
    video: ffmpeg::decoder::Video,
    video_filter: Option<PackedFrameFilter>,
    video_packets: VecDeque<ffmpeg::Packet>,
    input_eof: bool,
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
        let video_clock = CinematicFrameClock::new(&path, video_time_base, stream.rate())?;
        let mut parameters = stream.parameters();
        let video_filter = if parameters.id() == ffmpeg::codec::Id::MPEG4 {
            let filter =
                PackedFrameFilter::new(&parameters, video_time_base).map_err(|source| {
                    CinematicError::adapter(&path, "initialize MPEG-4 unpacker", source)
                })?;
            parameters = filter.parameters().map_err(|source| {
                CinematicError::adapter(&path, "read unpacked video parameters", source)
            })?;
            Some(filter)
        } else {
            None
        };
        let context = ffmpeg::codec::context::Context::from_parameters(parameters)
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
            video_clock,
            video,
            video_filter,
            video_packets: VecDeque::new(),
            input_eof: false,
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

    /// Decodes the next clocked tightly packed RGBA8 frame.
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
            if let Some(packet) = self.video_packets.pop_front() {
                self.video.send_packet(&packet).map_err(|source| {
                    CinematicError::adapter(&self.path, "submit video packet", source)
                })?;
                continue;
            }
            if self.input_eof {
                self.video.send_eof().map_err(|source| {
                    CinematicError::adapter(&self.path, "finish video stream", source)
                })?;
                self.eof_sent = true;
                continue;
            }
            let audio_stream_index = self.audio.as_ref().map(|audio| audio.stream_index);
            let packet = self.input.packets().find(|(stream, _packet)| {
                stream.index() == self.video_stream_index
                    || audio_stream_index == Some(stream.index())
            });
            if let Some((stream, packet)) = packet {
                if stream.index() == self.video_stream_index {
                    self.filter_video_packet(Some(packet))?;
                } else if let Some(audio) = self.audio.as_mut() {
                    audio.decoder.send_packet(&packet).map_err(|source| {
                        CinematicError::adapter(&self.path, "submit audio packet", source)
                    })?;
                    self.receive_audio_frames()?;
                }
            } else {
                self.filter_video_packet(None)?;
                if let Some(audio) = self.audio.as_mut() {
                    audio.decoder.send_eof().map_err(|source| {
                        CinematicError::adapter(&self.path, "finish audio stream", source)
                    })?;
                    self.receive_audio_frames()?;
                }
                self.input_eof = true;
            }
        }
    }

    fn filter_video_packet(
        &mut self,
        mut packet: Option<ffmpeg::Packet>,
    ) -> Result<(), CinematicError> {
        let Some(filter) = &mut self.video_filter else {
            self.video_packets.extend(packet);
            return Ok(());
        };
        filter.send(packet.as_mut()).map_err(|source| {
            CinematicError::adapter(&self.path, "submit packed video packet", source)
        })?;
        loop {
            match filter.receive() {
                Ok(packet) => self.video_packets.push_back(packet),
                Err(ffmpeg::Error::Other { errno }) if errno == EAGAIN => return Ok(()),
                Err(ffmpeg::Error::Eof) => return Ok(()),
                Err(source) => {
                    return Err(CinematicError::adapter(
                        &self.path,
                        "receive unpacked video packet",
                        source,
                    ));
                }
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
        let presentation_time = self.video_clock.resolve(&self.path, decoded.timestamp())?;
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
        CinematicVideoFrame::new(width, height, presentation_time, pixels).map_err(|()| {
            CinematicError::FrameSize {
                path: self.path.clone(),
                width,
                height,
            }
        })
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

#[cfg(test)]
mod tests {
    use super::CinematicFrameClock;
    use ffmpeg_next::Rational;
    use std::path::Path;
    use std::time::Duration;

    #[test]
    fn constant_rate_clock_advances_an_untimestamped_drain_frame()
    -> Result<(), super::CinematicError> {
        let path = Path::new("stock.avi");
        let mut clock = CinematicFrameClock::new(path, Rational(1, 24), Rational(24, 1))?;
        assert_eq!(
            clock.resolve(path, Some(4_755))?,
            Duration::from_secs_f64(4_755.0 / 24.0)
        );
        assert_eq!(
            clock.resolve(path, None)?,
            Duration::from_secs_f64(4_756.0 / 24.0)
        );
        Ok(())
    }

    #[test]
    fn constant_rate_clock_rejects_a_missing_initial_timestamp() -> Result<(), super::CinematicError>
    {
        let path = Path::new("stock.avi");
        let mut clock = CinematicFrameClock::new(path, Rational(1, 24), Rational(24, 1))?;
        assert!(clock.resolve(path, None).is_err());
        Ok(())
    }
}
