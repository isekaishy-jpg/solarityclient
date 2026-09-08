# Screenshots

The shared Lua `Screenshot()` binding sends a process action to the application
owner. Glue and FrameXML use the same renderer capture. A request copies the next
successfully presented framebuffer, including postprocessing, world geometry,
and UI. Normal presentation does not allocate readback storage or wait for it.

`RuntimeScreenshots` coalesces requests before presentation and retains one GPU
capture and one encoder task at most. Readback waits for that frame's GPU work;
image encoding and filesystem writes run on the owned CPU executor. Completion
dispatches the no-argument `SCREENSHOT_SUCCEEDED` or `SCREENSHOT_FAILED` event
to FrameXML, or `GLUE_SCREENSHOT_SUCCEEDED` / `GLUE_SCREENSHOT_FAILED` to Glue.
A save failure leaves the client running.

Files go to `Screenshots` under the configured profile root, alongside `WTF`.
The stock local-calendar name is `WoWScrnShot_MMDDYY_HHMMSS.jpg` or `.tga`.
An ordinal suffix preserves additional captures made in the same second.
Files are created exclusively, and an unsuccessful encoding removes only its
own incomplete output.

Build 12340 registers `screenshotFormat = jpeg` and `screenshotQuality = 3`
at 401EE4/401F0C. The setters retain string CVars. `76F0D0` parses the quality
as a wrapping decimal integer prefix; `4A84A0` clamps the unsigned result to
1–10 and truncates `45 + 5.5 * quality`. Thus the default JPEG quality is 61.
`4A8570` selects TGA for the case-insensitive `tga` token and JPEG otherwise.
`86D490` supplies the local date format. `4DC520` and `5150E0` schedule capture;
their callbacks at `4D8540` and `512E20` signal Glue/world completion events.

The image crate supplies the JPEG and TGA codecs; encoded bytes are not claimed
to match the original codecs. TGA retains exact framebuffer channels and row
order. The native quality oracle executes unchanged conversion functions for
26 CVar inputs. GPU tests decode both saved formats, check request lifetime and
save failures, and verify preservation of existing files. The application test
executes Lua through actual presentation, worker saving, and Lua completion for
both success and filesystem failure.
