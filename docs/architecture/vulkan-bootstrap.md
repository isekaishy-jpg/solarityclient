# Vulkan 1.3 bootstrap and presentation ownership

## Decision

Solarity has one rendering backend: Vulkan 1.3 through `ash`. SPIR-V remains
the shader interface. SDL owns the native window and supplies only the Vulkan
instance-extension list and native surface creation call. The rendering crate
owns every Vulkan object and the VMA allocator.

The split initialization is explicit:

1. rendering loads Vulkan 1.3 and creates an instance with exactly SDL's
   required extensions;
2. SDL creates a native surface from that instance;
3. ownership of the surface transfers immediately back to rendering;
4. rendering validates the explicitly indexed adapter and creates the logical
   device, queues, VMA allocator, swapchain, and image views.

The two calls that cross the safe Rust boundary state the handle and lifetime
invariants in `# Safety` documentation. All Ash and SDL unsafe blocks are local
to the platform/device implementation and carry an adjacent safety argument.
The workspace lint is `deny`, rather than `forbid`, so those reviewed modules
can opt in narrowly while accidental unsafe code elsewhere remains an error.

## Adapter policy

`--gpu-index` is required and zero-based. Rendering validates that exact
Vulkan enumeration entry. It never scans later adapters after a failure and
does not silently choose an integrated, discrete, software, or compatibility
device. This makes multi-GPU test runs and failures reproducible.

The selected adapter and SDL surface must provide:

- Vulkan API 1.3;
- `VK_KHR_swapchain`;
- dynamic rendering and synchronization2;
- core BC texture compression plus transferable, linearly sampled BC1/2/3
  UNORM and sRGB images;
- either one combined graphics/present queue family or one family of each;
- `B8G8R8A8_UNORM` with `SRGB_NONLINEAR` color space;
- FIFO presentation;
- opaque composition and color-attachment image usage.

The BGRA8 layout corresponds to the stock D3D backbuffer format family. FIFO
is deterministic and universally required by ordinary Vulkan window surfaces;
other presentation policies can be added only as explicit configuration, not
as silent fallbacks.

When the surface fixes its extent, that platform value is authoritative. For a
variable-extent surface, the SDL physical pixel extent is clamped to Vulkan's
reported legal bounds as required by the specification. Swapchain image count
is one above the surface minimum, capped by a nonzero maximum.

## Stock relationship

Build-12340 groups the analogous adapter, device, backbuffer, and presentation
responsibilities under `CGxDevice.cpp`, `CGxDeviceD3d.cpp`, the D3D9/D3D9Ex
implementations, and the OpenGL backend. Solarity preserves the ownership and
failure boundary while deliberately replacing those APIs with one 64-bit
Vulkan backend. This is not a claim that stock supported Vulkan.

## Lifetime order

Initialization failures immediately enter an owner with a valid drop path.
Partial image-view creation is therefore recoverable without leaking earlier
views. Normal and failed teardown use this order:

1. wait for the logical device to become idle;
2. destroy swapchain image views;
3. destroy the swapchain;
4. drop VMA after all VMA resources (none exist in the bootstrap commit);
5. destroy the logical device;
6. destroy the SDL-created surface;
7. destroy the Vulkan instance;
8. unload Vulkan after all function tables are gone.

Runtime tests create a real SDL window, instance, surface, explicitly selected
adapter, VMA allocator, and swapchain. They also submit an impossible adapter
index after orderly shutdown to prove there is no adapter fallback and that
the partial instance/surface path is reusable.
