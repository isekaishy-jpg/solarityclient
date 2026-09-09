//! Worker-owned H.264/AAC encoding and MP4 muxing through the existing FFmpeg build.

#![allow(unsafe_code)]

use std::ffi::CString;
use std::time::Duration;

use ffmpeg::{ChannelLayout, Dictionary, Packet, Rational, codec, encoder, format, frame};
use ffmpeg_next as ffmpeg;

use super::RecordingError;
use super::audio::AudioBlock;
use super::files::{AUTOMATED_FILES, RecordingFiles, RecordingMode, SEGMENT_SECONDS};

const FPS: u64 = 30;
const MAX_PACKET_BYTES: usize = 4 * 1024 * 1024;

/// All codec state stays on one worker; cached frames respect encoder reference ownership.
pub(super) struct RecordingEncoder {
    output: format::context::Output,
    video: encoder::Video,
    audio: encoder::Audio,
    scaler: ffmpeg::software::scaling::Context,
    picture: frame::Video,
    sound: frame::Audio,
    extent: (u32, u32),
    rate: u32,
    video_time: Rational,
    audio_time: Rational,
    audio_fill: usize,
    audio_frames: u64,
    pub(super) video_frames: u64,
    picture_ready: bool,
    first_segment: u64,
    pub(super) active_segment: u64,
}

impl RecordingEncoder {
    /// Open hardware encoding and fragmented output before accepting capture samples.
    pub(super) fn create(
        files: &RecordingFiles,
        extent: (u32, u32),
        rate: u32,
    ) -> Result<Self, RecordingError> {
        ffmpeg::init()?;
        let video_codec = encoder::find_by_name("h264_mf").ok_or_else(|| {
            RecordingError::new("the bundled FFmpeg has no Media Foundation H.264 encoder")
        })?;
        let audio_codec = encoder::find(codec::Id::AAC)
            .ok_or_else(|| RecordingError::new("AAC encoder is unavailable"))?;
        let mut video = codec::context::Context::new_with_codec(video_codec)
            .encoder()
            .video()?;
        video.set_width(extent.0);
        video.set_height(extent.1);
        video.set_format(format::Pixel::NV12);
        video.set_time_base((1, FPS as i32));
        video.set_frame_rate(Some((FPS as i32, 1)));
        video.set_bit_rate(4_000_000);
        video.set_gop(FPS as u32);
        video.set_max_b_frames(0);
        video.set_flags(codec::Flags::GLOBAL_HEADER);
        // SAFETY: This uniquely owned, unopened codec context receives standard
        // BT.709 limited-range metadata matching the scaler configured below.
        unsafe {
            (*video.as_mut_ptr()).colorspace = ffmpeg::ffi::AVColorSpace::AVCOL_SPC_BT709;
            (*video.as_mut_ptr()).color_primaries = ffmpeg::ffi::AVColorPrimaries::AVCOL_PRI_BT709;
            (*video.as_mut_ptr()).color_trc =
                ffmpeg::ffi::AVColorTransferCharacteristic::AVCOL_TRC_BT709;
            (*video.as_mut_ptr()).color_range = ffmpeg::ffi::AVColorRange::AVCOL_RANGE_MPEG;
        }
        let mut options = Dictionary::new();
        options.set("hw_encoding", "1");
        options.set("rate_control", "cbr");
        options.set("scenario", "camera_record");
        let video = video.open_with(options).map_err(|error| {
            RecordingError::new(format!("hardware H.264 encoder could not start: {error}"))
        })?;
        let mut audio = codec::context::Context::new_with_codec(audio_codec)
            .encoder()
            .audio()?;
        audio.set_rate(rate as i32);
        audio.set_channel_layout(ChannelLayout::STEREO);
        audio.set_format(format::Sample::F32(format::sample::Type::Planar));
        audio.set_bit_rate(128_000);
        audio.set_time_base((1, rate as i32));
        audio.set_flags(codec::Flags::GLOBAL_HEADER);
        let audio = audio.open_as(audio_codec)?;
        let mut output = recording_output(files)?;
        {
            let mut stream = output.add_stream(video_codec)?;
            stream.set_time_base((1, FPS as i32));
            stream.set_parameters(&video);
        }
        {
            let mut stream = output.add_stream(audio_codec)?;
            stream.set_time_base((1, rate as i32));
            stream.set_parameters(&audio);
        }
        let mut options = Dictionary::new();
        if files.mode == RecordingMode::Automated {
            options.set("segment_time", &SEGMENT_SECONDS.to_string());
            options.set("segment_wrap", &AUTOMATED_FILES.to_string());
            options.set("segment_start_number", &files.first_segment.to_string());
            options.set("segment_format", "mp4");
            options.set("reset_timestamps", "1");
            options.set(
                "segment_format_options",
                "movflags=+frag_keyframe+delay_moov+default_base_moof",
            );
        } else {
            options.set("movflags", "+frag_keyframe+delay_moov+default_base_moof");
        }
        drop(output.write_header_with(options)?);
        let video_time = output
            .stream(0)
            .ok_or_else(|| RecordingError::new("missing video stream"))?
            .time_base();
        let audio_time = output
            .stream(1)
            .ok_or_else(|| RecordingError::new("missing audio stream"))?
            .time_base();
        let mut scaler = ffmpeg::software::scaling::Context::get(
            format::Pixel::BGRA,
            extent.0,
            extent.1,
            format::Pixel::NV12,
            extent.0,
            extent.1,
            ffmpeg::software::scaling::Flags::BILINEAR,
        )?;
        // SAFETY: Static coefficient table and live scaler; source is full-range
        // RGB and output is limited-range BT.709 YUV, with neutral adjustments.
        let result = unsafe {
            let coefficients = ffmpeg::ffi::sws_getCoefficients(ffmpeg::ffi::SWS_CS_ITU709);
            ffmpeg::ffi::sws_setColorspaceDetails(
                scaler.as_mut_ptr(),
                coefficients,
                1,
                coefficients,
                0,
                0,
                1 << 16,
                1 << 16,
            )
        };
        if result < 0 {
            return Err(ffmpeg::Error::from(result).into());
        }
        let mut sound = frame::Audio::new(
            audio.format(),
            audio.frame_size() as usize,
            ChannelLayout::STEREO,
        );
        sound.set_rate(rate);
        let mut picture = frame::Video::new(format::Pixel::NV12, extent.0, extent.1);
        picture.set_color_space(ffmpeg::color::Space::BT709);
        picture.set_color_range(ffmpeg::color::Range::MPEG);
        picture.set_color_primaries(ffmpeg::color::Primaries::BT709);
        picture.set_color_transfer_characteristic(ffmpeg::color::TransferCharacteristic::BT709);
        Ok(Self {
            output,
            video,
            audio,
            scaler,
            picture,
            sound,
            extent,
            rate,
            video_time,
            audio_time,
            audio_fill: 0,
            audio_frames: 0,
            video_frames: 0,
            picture_ready: false,
            first_segment: files.first_segment,
            active_segment: files.first_segment,
        })
    }

    /// Hold the previous image through missing samples, then convert a new BGRA image.
    pub(super) fn image(
        &mut self,
        pixels: &[u8],
        timestamp: Duration,
    ) -> Result<(), RecordingError> {
        let index = (timestamp.as_nanos() * u128::from(FPS) / 1_000_000_000) as u64;
        if self.picture_ready && index < self.video_frames {
            return Ok(());
        }
        self.video_until(index)?;
        let row = self.extent.0 as usize * 4;
        if pixels.len() != row * self.extent.1 as usize {
            return Err(RecordingError::new("recording pixel size changed"));
        }
        // SAFETY: Make the cached frame writable before changing pixels that a
        // delayed encoder may still retain. The source and destination extents,
        // plane pointers, and strides match their declared formats exactly.
        unsafe {
            let writable = ffmpeg::ffi::av_frame_make_writable(self.picture.as_mut_ptr());
            if writable < 0 {
                return Err(ffmpeg::Error::from(writable).into());
            }
            let source = [
                pixels.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
            ];
            let strides = [row as i32, 0, 0, 0];
            let destination = self.picture.as_mut_ptr();
            let rows = ffmpeg::ffi::sws_scale(
                self.scaler.as_mut_ptr(),
                source.as_ptr(),
                strides.as_ptr(),
                0,
                self.extent.1 as i32,
                (*destination).data.as_ptr(),
                (*destination).linesize.as_ptr(),
            );
            if rows != self.extent.1 as i32 {
                return Err(RecordingError::new("video color conversion was incomplete"));
            }
        }
        self.picture_ready = true;
        self.video_until(index.saturating_add(1))
    }

    /// Emit constant-rate frames from the retained picture up to the exclusive index.
    pub(super) fn video_until(&mut self, exclusive_frame: u64) -> Result<(), RecordingError> {
        if !self.picture_ready {
            return Ok(());
        }
        while self.video_frames < exclusive_frame {
            self.picture.set_pts(Some(self.video_frames as i64));
            self.video.send_frame(&self.picture)?;
            self.video_frames += 1;
            self.drain_video()?;
        }
        Ok(())
    }

    /// Preserve source timing with silence for missing samples and trim overlapping blocks.
    pub(super) fn audio(&mut self, block: AudioBlock) -> Result<(), RecordingError> {
        let current = self.audio_frames + self.audio_fill as u64;
        if block.first_frame > current {
            self.silence_until(block.first_frame)?;
        }
        let skip = (self.audio_frames + self.audio_fill as u64)
            .saturating_sub(block.first_frame)
            .min(block.frames as u64) as usize;
        for pair in block.samples[skip * 2..block.frames * 2]
            .as_chunks::<2>()
            .0
            .iter()
        {
            self.sample(pair[0], pair[1])?;
        }
        Ok(())
    }

    /// Fill missing source-clock positions without changing subsequent audio timing.
    pub(super) fn silence_until(&mut self, sample: u64) -> Result<(), RecordingError> {
        while self.audio_frames + (self.audio_fill as u64) < sample {
            self.sample(0.0, 0.0)?;
        }
        Ok(())
    }

    /// Fill cached planar AAC input; non-finite mixer samples become silence.
    fn sample(&mut self, left: f32, right: f32) -> Result<(), RecordingError> {
        if self.audio_fill == 0 {
            // SAFETY: Encoder references may outlive send_frame. Restore exclusive
            // writable planes before filling the next fixed-size audio frame.
            let result = unsafe { ffmpeg::ffi::av_frame_make_writable(self.sound.as_mut_ptr()) };
            if result < 0 {
                return Err(ffmpeg::Error::from(result).into());
            }
        }
        self.sound.plane_mut::<f32>(0)[self.audio_fill] = if left.is_finite() { left } else { 0.0 };
        self.sound.plane_mut::<f32>(1)[self.audio_fill] =
            if right.is_finite() { right } else { 0.0 };
        self.audio_fill += 1;
        if self.audio_fill == self.audio.frame_size() as usize {
            self.flush_audio_frame()?;
        }
        Ok(())
    }

    /// Submit a full or final partial audio frame and release available packets.
    fn flush_audio_frame(&mut self) -> Result<(), RecordingError> {
        if self.audio_fill == 0 {
            return Ok(());
        }
        self.sound.set_samples(self.audio_fill);
        self.sound.set_pts(Some(self.audio_frames as i64));
        self.audio.send_frame(&self.sound)?;
        self.audio_frames += self.audio_fill as u64;
        self.audio_fill = 0;
        self.sound.set_samples(self.audio.frame_size() as usize);
        self.drain_audio()
    }

    /// Mux available video packets and track the currently owned automated segment.
    fn drain_video(&mut self) -> Result<(), RecordingError> {
        let mut packet = Packet::empty();
        loop {
            match self.video.receive_packet(&mut packet) {
                Ok(()) => {
                    if let Some(pts) = packet.pts() {
                        self.active_segment = (self.first_segment
                            + pts.max(0) as u64 / (FPS * SEGMENT_SECONDS))
                            % AUTOMATED_FILES;
                    }
                    packet.set_stream(0);
                    if packet.duration() == 0 {
                        packet.set_duration(1);
                    }
                    packet.rescale_ts((1, FPS as i32), self.video_time);
                    self.write_packet(&mut packet)?;
                }
                Err(ffmpeg::Error::Eof) => break,
                Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::util::error::EAGAIN => {
                    break;
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    /// Mux AAC packets while preserving the encoder-provided priming timestamps.
    fn drain_audio(&mut self) -> Result<(), RecordingError> {
        let mut packet = Packet::empty();
        loop {
            match self.audio.receive_packet(&mut packet) {
                Ok(()) => {
                    packet.set_stream(1);
                    packet.rescale_ts((1, self.rate as i32), self.audio_time);
                    self.write_packet(&mut packet)?;
                }
                Err(ffmpeg::Error::Eof) => break,
                Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::util::error::EAGAIN => {
                    break;
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    /// Bound unusually large packets before writing through the interleaving muxer.
    fn write_packet(&mut self, packet: &mut Packet) -> Result<(), RecordingError> {
        if packet.size() > MAX_PACKET_BYTES {
            return Err(RecordingError::new(
                "encoded packet exceeds recording disk-budget headroom",
            ));
        }
        packet.write_interleaved(&mut self.output)?;
        Ok(())
    }

    /// Complete the requested timeline, drain delayed packets, and close the MP4 index.
    pub(super) fn finish(&mut self, elapsed: Duration) -> Result<(), RecordingError> {
        let frames = (elapsed.as_nanos() * u128::from(FPS)).div_ceil(1_000_000_000) as u64;
        self.video_until(frames)?;
        self.silence_until((elapsed.as_nanos() * u128::from(self.rate) / 1_000_000_000) as u64)?;
        self.flush_audio_frame()?;
        self.video.send_eof()?;
        self.drain_video()?;
        self.audio.send_eof()?;
        self.drain_audio()?;
        self.output.write_trailer()?;
        Ok(())
    }
}

/// Segment muxing owns its files. The wrapper's output_as always opens the
/// supplied path, which would create or truncate the literal filename pattern.
fn recording_output(files: &RecordingFiles) -> Result<format::context::Output, RecordingError> {
    if files.mode == RecordingMode::Manual {
        return Ok(format::output_as(&files.path, "mp4")?);
    }
    let filename = CString::new(
        files
            .path
            .to_str()
            .ok_or_else(|| RecordingError::new("recording path is not valid UTF-8"))?,
    )
    .map_err(RecordingError::new)?;
    let mut context = std::ptr::null_mut();
    // SAFETY: FFmpeg copies these live C strings into a newly allocated context.
    // The segment muxer is AVFMT_NOFILE and opens only its numbered outputs.
    let result = unsafe {
        ffmpeg::ffi::avformat_alloc_output_context2(
            &mut context,
            std::ptr::null_mut(),
            c"segment".as_ptr(),
            filename.as_ptr(),
        )
    };
    if result < 0 {
        return Err(ffmpeg::Error::from(result).into());
    }
    if context.is_null() {
        return Err(RecordingError::new(
            "segment muxer did not allocate an output context",
        ));
    }
    // SAFETY: Sole ownership of the valid context transfers to the normal output
    // destructor. Its null main AVIO is valid because segments own all file I/O.
    Ok(unsafe { format::context::Output::wrap(context) })
}
