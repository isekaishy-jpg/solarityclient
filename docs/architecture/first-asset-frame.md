# First asset-backed frame

## Purpose

The first presented frame proves the complete dependency chain before UI or
world rendering is built:

`MPQ priority -> BLP parser -> RGBA8 pixels -> VMA staging -> Vulkan transfer -> presentation`

Runtime resolves `Interface\Icons\INV_Misc_QuestionMark.blp`, the stock missing
icon used throughout the client. This is a fixed bootstrap diagnostic asset,
not a missing-file fallback: failure to resolve or decode it aborts startup.
The normal UI composition will supersede this screen.

## Resource sizing

BLP width and height come exclusively from the selected archive entry. The
asset layer does not impose the 64-by-64 dimensions of the ordinary client
icon. A patch or HD pack that replaces the same virtual path with a larger BLP
therefore allocates and composes the larger decoded resource automatically.

`wow-blp` parses BLP1/BLP2 encodings and produces the top mip as owned RGBA8.
The asset facade retains the exact winning archive descriptor without exposing
the decoder's types. Malformed content becomes a stable `TextureDecode` error.

## Bootstrap presentation

This first proof does not introduce a temporary shader or pipeline that later
rendering would need to discard. Rendering performs a one-shot transfer:

1. preserve aspect ratio and nearest-neighbor compose the decoded RGBA8 texture
   into a black frame at the physical swapchain extent;
2. convert channels to the required `B8G8R8A8_UNORM` swapchain layout;
3. allocate an exact-size host-visible, host-coherent VMA staging buffer;
4. acquire one swapchain image;
5. transition `UNDEFINED -> TRANSFER_DST_OPTIMAL` with synchronization2;
6. copy the tightly packed frame and transition to `PRESENT_SRC_KHR`;
7. submit with explicit acquire/transfer semaphores, present, and retire the
   graphics submission fence;
8. destroy the transient buffer, command pool, semaphores, and fence;
9. reveal the SDL window only after presentation succeeds.

The normal shutdown still waits for the entire device, including the present
queue, to become idle before destroying the swapchain. A frame fence alone is
not treated as evidence that presentation has retired.

## Evidence

The external runtime fixture supplies a two-pixel BLP2/RAW3 file through the
localized MPQ and asserts the presented source extent. The local validation run
uses the real 64-by-64 `enUS` icon, creates a 1280-by-720 three-image swapchain
on Vulkan adapter zero, submits the transfer, presents it, and shuts down
cleanly. No client asset bytes are committed.
