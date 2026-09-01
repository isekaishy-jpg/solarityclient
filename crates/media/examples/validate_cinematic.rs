//! Decodes a bounded number of frames from one installed stock AVI.

use std::error::Error;
use std::path::PathBuf;

use solarity_media::CinematicDecoder;

fn main() -> Result<(), Box<dyn Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: validate_cinematic <movie.avi> [frame-count]")?;
    let count = std::env::args()
        .nth(2)
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(3);
    let mut decoder = CinematicDecoder::open(&path)?;
    for index in 0..count {
        let frame = decoder
            .next_video_frame()?
            .ok_or("movie ended before the requested validation frame count")?;
        println!(
            "frame={index} extent={}x{} pts_ms={} bytes={}",
            frame.width(),
            frame.height(),
            frame.presentation_time().as_millis(),
            frame.rgba8().len()
        );
    }
    Ok(())
}
