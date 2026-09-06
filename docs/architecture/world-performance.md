# Offline World performance replay

`solarity-runtime`'s `benchmark_world` example exercises the production terrain
streaming, environment, local-player presentation, FrameXML, camera collision,
M2/WMO presentation, and Vulkan owners without authenticating or connecting to a
realm. The caller provides a map and position. The example supplies a level-one
human warrior with empty equipment and a fixed noon realm clock.

The command takes `frames-per-phase output.csv map x y z` followed by the ordinary
runtime configuration arguments. Use an isolated `--profile-root` and explicitly
select window dimensions, presentation mode, and GPU. CSV rows retain each
measured frame, including terrain publication and hover stalls. Required assets,
FrameXML callbacks, and recoverable presentation failures terminate the replay;
they do not become skipped samples.

The four phases request initial streaming, an unchanged view, a full camera
orbit, and pointer motion through FrameXML. Streaming remains active in every
phase, so the `resident_tiles` column must be inspected before describing an
interval as settled. The phase names describe input, not a promise that resource
loading has finished. Samples separate service, streaming, UI, final camera
resolution, and presentation. Streaming currently includes a camera resolution
of its own. `SOLARITY_FRAME_TIMINGS=1` additionally reports the renderer's resource,
buffer/fence, image acquisition, command-recording, submit, and present intervals.
It also aggregates camera resolutions by terrain, placed WMO, placed M2, and
terrain/WMO liquid provider. Each resolution retains all nine obstruction probes.

This replay does not exercise the network, movement solver, remote population,
audio, or diagnostic overlays. Its frame times are evidence about the exercised
production paths, not a substitute for measurements from a populated live world.
