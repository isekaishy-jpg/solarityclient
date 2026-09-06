# Unit portrait rendering

`SetPortraitTexture` now publishes a typed texture source through the Lua
snapshot, retained presentation, mesh plan, and renderer. `SetTexture` clears
that request when selecting a file or a solid color. An unresolved unit clears
the image and returns false; the current world UI unit resolver admits `player`.

The runtime joins the request to the complete resident player appearance. It
checks the publication generation before sampling, then uses the resident body,
selected geosets, character atlas, item models, and item visual geometry. The
portrait has its own bone palettes and material packets. It reads no live
animation clock and advances no live particle simulation or random stream.
Appearance changes recapture the image; ordinary world frames reuse it.
Initial portrait preparation runs behind the loading card once the published
player and scene are available.

Build 12340's `0x00619580` clones the unit model, and `0x004EAF70` resets its
transform and detaches transient attachment points. The retained equipment
points include weapons, helmet, shoulders, back, and hip attachments. Solarity
reconstructs that retained attachment hierarchy around an identity body
transform. The body requests Stand, variation ordinal zero, at time zero.

Camera selection uses semantic camera lookup slot zero (`0x00827960`), rather
than assuming that the first authored camera has the portrait role. Without
that slot, `0x0082CED0` supplies the selected sequence's bounding center and
radius. The fallback eye is 1.7 radii along +X, with diagonal FOV 1.57, near clip
1/36, and the camera constructor's 5,000-unit far clip. `0x00616BC0` supplies
ambient RGB 0.45 and a white directional light with D3D ray (-1, 0, -1).
Portrait material packets explicitly disable world fog. Zeroing scene fog
parameters alone does not disable the material's fog shader path.

The Vulkan renderer retains one 64 by 64 color attachment per unit token and
reuses the existing M2 pipeline and frame-buffer ABI. A second color-only pass
samples `Interface/CharacterFrame/TempPortraitAlphaMask.blp` and replaces only
destination alpha. It preserves model RGB, including the black background.
The result stays on the GPU and enters ordinary UI composition through a typed
portrait texture handle. No framebuffer readback is used in this path.

All portrait submissions use the renderer's graphics queue. Reuse waits for the
portrait frame slot; an execution barrier orders earlier UI fragment reads
before overwriting the sampled image, and the final image barrier exposes color
writes to subsequent fragment sampling. Image handles and their descriptors
survive appearance changes. Renderer teardown retires GPU work before releasing
the frame slot, views, and images.

GPU regression coverage checks asymmetric mask alpha, preserved model RGB,
repeated sampling and overwriting of the same image, separate unit targets, and
queued teardown. UI coverage switches among portrait, file, solid color, and
missing-unit sources through FrameXML bindings. The equipped-unit regression
checks that portrait capture and recapture preserve live component timers and
effect histories, and checks that the composed image contains visible geometry.
The real HumanMale model was also captured in the Drag at 2560 by 1440; its
authored portrait camera frames the face in both PlayerFrame and the character
micro button, with the archive's circular mask. This capture verifies the real
asset path rather than relying solely on the synthetic GPU fixtures.

An uncaptured 1,800-frame-per-phase replay with portraits enabled measured
7.338 ms stationary, 7.127 ms orbit, and 7.732 ms pointer means, compared with
7.269, 7.074, and 7.621 ms before portraits. No compilation ran during that replay.
The settled means remain around 129–140 FPS in this offline 1440p scene; this
does not establish live-server performance or the requested 1,200 FPS target.

Remaining unit-token integration follows the world's authoritative unit resolver.
The native temporary race/gender portrait fallback and non-player unit portrait
requests are not implemented by this player-residency path.
