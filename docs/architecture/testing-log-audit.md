# Testing log audit: build 34

The seven September 8, 2026 testing logs contained four issue classes. The
latest run ended with the appearance error below; the client was not closed
by an external process.

| Log item | Cause and disposition |
| --- | --- |
| `character race 2 gender 0 has no skin section at variation 0 color 17` | Creation eligibility was incorrectly applied to rendering. The component bank now accepts every authored section, using the final physical row for duplicate keys. |
| `Data is smaller than expected` / `Reading 1 blocks` | Authored narrow DXT tail mips. The asset boundary now validates their one-block representation explicitly and preserves every block of complete narrow images. |
| MPEG-4 packed B-frame warning | Cinematic packets now pass through FFmpeg's `mpeg4_unpack_bframes` filter before video decoding. |
| Lua `Screenshot` was nil | Already corrected by the screenshot API and GPU capture implementation shipped in build 34. No recurrence in the latest run. |

Across these logs there were 102 pairs of DXT warnings, two packed-frame
warnings, seven old screenshot errors, and one appearance failure.

## Character evidence

The unchanged build-12340 functions `0x004F3DD0` and `0x004F3BA0` construct and
query the rendering bank without a class or eligibility filter. Only allocation
is replaced in the capture harness. The committed native fixture covers 160
lookups across all five sections and flag values 0 through 31. A separate run
against the installed `CharSections.dbc` returns Orc male skin/face/underwear
rows 10627, 10628, and 10631 for skin 17. The ECS regression includes a remote
Orc warrior carrying that skin. Creation-choice eligibility remains separate.

## Texture evidence and boundary

An audit of 111,072 installed BLP paths found 1,097 undersized compressed mips
in 866 textures. Every case has a one- or two-pixel smaller axis and exactly
one whole compressed block. All 866 textures load with the corrected boundary.
`validate_texture_mips` can repeat the scan or take a newline-separated path
list as its second argument.

The parser validates file ranges, complete compression blocks, and nonzero
bounded dimensions. Incomplete ordinary mips and partial blocks are errors.
The existing CPU/GPU zero-padding treatment of short authored tails is retained;
this audit does not establish pixel parity with stock's handling of missing
blocks. Complete narrow mips now retain their full block-rounded payload:
rounding total pixel area, as the dependency did, can silently lose blocks.
BC1, BC2, and BC3 pixel tests and actual Vulkan upload tests pass.

## Cinematic evidence and remaining source diagnostic

The filter owns its FFmpeg context and buffered packet references, copies its
output codec parameters into the decoder, and drains output before accepting
more input. End of input drains the filter before the video decoder. Tests
exercise packed-frame splitting, authored packet timestamps, and EOF. This
follows the [FFmpeg bitstream filter API](https://ffmpeg.org/doxygen/8.0/group__lavc__bsf.html)
and [MPEG-4 unpacker](https://www.ffmpeg.org/ffmpeg-bitstream-filters.html#mpeg4_005funpack_005fbframes).

The installed `WOW_Intro_LK_1024.avi` completes with 4,757 video frames, a final
presentation time of 198,208 ms, and 17,482,650 interleaved audio samples. The
packed-frame warning is absent. Full playback exposes an additional
`mp3float: invalid new backstep -1` diagnostic at the very end. Independent
FFmpeg audio-only decoding reproduces it without the cinematic player,
resampler, or video filter. The final MP3 packet is 327 bytes; its `FF FB A4 64`
header declares a 480-byte MPEG-1 Layer III frame. This is a truncated source
frame. FFmpeg recovers and playback completes. The source file and decoder
logging remain unchanged; this is not reported as a repaired asset.
