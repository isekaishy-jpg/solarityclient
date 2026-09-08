//! Owned FFmpeg MPEG-4 packet filter, isolated from cinematic scheduling.
#![allow(unsafe_code)]

use ffmpeg::codec::packet::Mut;
use ffmpeg::ffi;
use ffmpeg_next as ffmpeg;
use std::ptr::NonNull;

/// Unpacks DivX B-frames before decoding, preserving the authored packets' clock.
pub(super) struct PackedFrameFilter(NonNull<ffi::AVBSFContext>);

impl PackedFrameFilter {
    pub(super) fn new(
        parameters: &ffmpeg::codec::Parameters,
        time_base: ffmpeg::Rational,
    ) -> Result<Self, ffmpeg::Error> {
        // SAFETY: The filter name is static and NUL terminated. Allocation
        // initializes an exclusively owned context, released by Drop on every path.
        let raw = unsafe {
            let filter = ffi::av_bsf_get_by_name(c"mpeg4_unpack_bframes".as_ptr());
            if filter.is_null() {
                return Err(ffmpeg::Error::BsfNotFound);
            }
            let mut raw = std::ptr::null_mut();
            check(ffi::av_bsf_alloc(filter, &mut raw))?;
            raw
        };
        let owner = Self(NonNull::new(raw).ok_or(ffmpeg::Error::Bug)?);
        // SAFETY: Both parameter structures are live, distinct allocations;
        // FFmpeg copies the input and owns its output throughout this context.
        unsafe {
            check(ffi::avcodec_parameters_copy(
                (*raw).par_in,
                parameters.as_ptr(),
            ))?;
            (*raw).time_base_in = time_base.into();
            check(ffi::av_bsf_init(raw))?;
        }
        Ok(owner)
    }

    pub(super) fn parameters(&self) -> Result<ffmpeg::codec::Parameters, ffmpeg::Error> {
        let mut parameters = ffmpeg::codec::Parameters::new();
        // SAFETY: Copying from the live filter output into a separate owned
        // parameter object avoids exposing a borrow beyond the filter lifetime.
        unsafe {
            check(ffi::avcodec_parameters_copy(
                parameters.as_mut_ptr(),
                self.0.as_ref().par_out,
            ))?;
        }
        Ok(parameters)
    }

    pub(super) fn send(
        &mut self,
        packet: Option<&mut ffmpeg::Packet>,
    ) -> Result<(), ffmpeg::Error> {
        // SAFETY: Mutable access prevents concurrent operations. FFmpeg takes
        // and clears a packet only on success; null signals EOF once input ends.
        unsafe {
            let packet = packet.map_or(std::ptr::null_mut(), |packet| packet.as_mut_ptr());
            check(ffi::av_bsf_send_packet(self.0.as_ptr(), packet))
        }
    }

    pub(super) fn receive(&mut self) -> Result<ffmpeg::Packet, ffmpeg::Error> {
        let mut packet = ffmpeg::Packet::empty();
        // SAFETY: The empty destination owns any returned packet reference.
        // It is dropped normally on success or error, including EAGAIN/EOF.
        unsafe {
            check(ffi::av_bsf_receive_packet(
                self.0.as_ptr(),
                packet.as_mut_ptr(),
            ))?;
        }
        Ok(packet)
    }
}

impl Drop for PackedFrameFilter {
    fn drop(&mut self) {
        // SAFETY: This owner is never cloned and exclusively releases the
        // context plus any buffered packet references exactly once.
        unsafe {
            ffi::av_bsf_free(&mut self.0.as_ptr());
        }
    }
}

fn check(result: i32) -> Result<(), ffmpeg::Error> {
    if result < 0 {
        Err(ffmpeg::Error::from(result))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_b_frames_are_split_into_their_authored_packet_slots() -> Result<(), ffmpeg::Error> {
        ffmpeg::init()?;
        let mut parameters = ffmpeg::codec::Parameters::new();
        parameters.set_id(ffmpeg::codec::Id::MPEG4);
        parameters.set_medium(ffmpeg::media::Type::Video);
        let mut filter = PackedFrameFilter::new(&parameters, ffmpeg::Rational(1, 24))?;
        let mut header = ffmpeg::Packet::copy(b"\x00\x00\x01\xb2DivX503b1393p\0");
        filter.send(Some(&mut header))?;
        assert_eq!(
            filter.receive()?.data(),
            Some(&b"\x00\x00\x01\xb2DivX503b1393\0\0"[..])
        );
        assert!(
            matches!(filter.receive(), Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::util::error::EAGAIN)
        );
        let first = b"\x00\x00\x01\xb6first-frame-data";
        let second = b"\x00\x00\x01\xb6second-frame-data";
        let mut packed = ffmpeg::Packet::copy(&[first.as_slice(), second.as_slice()].concat());
        packed.set_pts(Some(1));
        filter.send(Some(&mut packed))?;
        let output = filter.receive()?;
        assert_eq!(output.data(), Some(first.as_slice()));
        assert_eq!(output.pts(), Some(1));
        assert!(filter.receive().is_err());
        let mut placeholder = ffmpeg::Packet::copy(b"\x00\x00\x01\xb6\x7f");
        placeholder.set_pts(Some(2));
        filter.send(Some(&mut placeholder))?;
        let output = filter.receive()?;
        assert_eq!(output.data(), Some(second.as_slice()));
        assert_eq!(output.pts(), Some(2));
        assert!(filter.receive().is_err());
        filter.send(None)?;
        assert!(matches!(filter.receive(), Err(ffmpeg::Error::Eof)));
        Ok(())
    }
}
