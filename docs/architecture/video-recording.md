# Video recording

Press **F9** to start recording the game window with its mixed game sound; press
it again to finish. `REC` appears beside the FPS display, followed by `SAVING`
and `VIDEO SAVED` or `VIDEO FAILED`. Recordings are MP4 files in the configured
profile's `Videos` directory. Manual files receive unique names and are retained.
This is an intentional Solarity diagnostic feature; F9 is consumed before game
bindings, including repeats and key release.

Capture is fixed at 30 FPS, preserves aspect ratio, and scales down to at most
1280 by 720. Resizing keeps the recording's original output dimensions and adds
black borders when necessary. It includes the final game image and UI, before
desktop composition and display gamma. It records game sound, without desktop
audio or a microphone. A minimized window holds the last image.

The existing FFmpeg dependency provides hardware H.264 through Windows Media
Foundation (`h264_mf`, hardware encoding required) at a target 4 Mbps, plus stereo
AAC at 128 kbps. Unsupported hardware reports failure. The encoder runs on one
owned worker while recording. There is no software H.264 fallback. Fragmented
MP4 retains completed fragments if a process crashes; orderly stopping flushes
the final fragment. Delayed MP4 initialization preserves AAC priming timing.

One reusable GPU readback slot has its own nonblocking completion fence. GPU
scaling bounds CPU readback to 720p; three recycled BGRA buffers bound the video
queue. The existing SDL mixer supplies final floating-point stereo samples to a
bounded queue. Its callback allocates no memory, performs no file I/O, and never
waits for the encoder. Congestion skips image samples or audio capture samples;
the encoder holds images or fills audio gaps with silence while retaining the
original timestamps. A sample-rate or channel-format change stops recording.
Screenshots retain priority over competing video samples.

When recording is off there is no readback allocation, encoder thread, audio
tap, or recurring recording clock work. Recording still costs GPU transfer and
hardware encode work, CPU color conversion and AAC encoding, and disk writes.

## Automated checks

Pass `--record-video` to the runtime or its Glue/World benchmark examples to
capture an automated run. This opt-in writes to `Videos/Automated`. The World
benchmark intentionally does not simulate world audio, so its recording's audio
track is silent. Normal gameplay and the Glue benchmark use the game mixer.

Glue replay FPS is a diagnostic measurement, not interactive client FPS. The
replay omits the interactive loop's session servicing and 1,200 FPS pacing;
resolution and presentation settings also follow the diagnostic profile. Compare
recording on/off within the same run configuration. Comparisons with Testing
require matching its actual pixel extent and measuring the interactive loop.

Automated output rotates sixty-second segments across eight owned files,
`check-00.mp4` through `check-07.mp4`. Rotation also advances across separate
short runs. A 256 MiB budget includes an 8 MiB write reserve; once per second,
older owned clips are pruned while the current clip is retained. Budget failure
stops recording. A filesystem lock excludes simultaneous writers to that folder.
Retention never removes manual recordings or unrelated filenames. At the target
bitrates a minute is approximately 31 MB.

Tests cover GPU pixels, scaling, reuse, resizing and screenshot arbitration;
audio callback removal and queue pressure; and file ownership and disk retention.
Explicit hardware tests decode H.264/AAC to verify colors, audio samples and
30 FPS timing, and fast-forward over eight minutes to exercise actual MP4
segment wrapping. Run the hardware tests with:

```text
cargo test -p solarity-media --release --lib recording::tests::hardware -- --ignored --nocapture
```
