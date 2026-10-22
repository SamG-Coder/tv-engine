# TCL Rust Vulkan renderer

A small native Rust opaque forward renderer running on the TCL V620 / Mali-G31. Vulkan is the only graphics backend. Opening **Rust 3D** starts the interactive island scene at 3840x2160. Arrow keys walk/turn, OK boosts movement, Back exits. Joystick axes are supported; physical gamepad testing is still outstanding.

## Build and run

```powershell
./build.ps1                       # Verified TCL Cortex-A55 target
./build.ps1 -Cpu generic          # Portable ARMv7 code generation
adb -s 192.168.50.35:5555 install -r Rust3D.apk
adb -s 192.168.50.35:5555 shell am start -n dev.tcl.rustengine/.VulkanActivity
```

Requires Rust with armv7-linux-androideabi, Android SDK/NDK (tested 28.2.13676358), and Android Studio JBR. The script compiles GLSL to optimized SPIR-V at build time, compiles Rust, and packages a development-signed APK. No runtime shader compiler, GLES backend, or large engine dependency. Ash provides Vulkan bindings; the installed system Vulkan loader provides the Mali driver.

Launch from Adult mode. The EducationGuard is not permanently bypassed or changed by the application.

## Renderer layout

- `src/geometry.rs`: backend-independent Vertex and Instance data; full-precision positions, colours, model matrices and tint.
- `src/math.rs`: camera, matrix and conservative sphere-frustum tests.
- `src/visibility.rs`: reusable visibility workspace; cached world bounds, compact distance/index sorting and preallocated output. Recreate the workspace when transforms change; an incremental animated-scene update API is not implemented yet.
- `src/vk_renderer.rs`: reusable Mesh, Buffer, Depth, Camera and Renderer types. Accepts arbitrary triangle-list meshes, deduplicates complete vertex attributes, chooses 16-bit indices when possible and otherwise uses 32-bit indices. Indexed instance draws take mesh ranges and instance offsets. GPU resources are owned and released on the render thread.
- `src/vulkan.rs`: Android surface/device/swapchain ownership, two reusable frame slots and the example scene loop.
- `src/scene.rs`: island/stress-scene generation, separate from the rendering backend.
- `shaders/`: opaque vertex-colour rendering, distance fog, and water/bobbing material variants. Pipeline specialization removes unused vertex-shader paths.
- `android/.../VulkanActivity.java`: surface lifetime and remote/joystick input. Vulkan is the sole launcher activity.

The unsafe rendering API requires a live device, compatible render pass, recording command buffer, and completed frame fence before updating mapped buffers. Keep all dependent resources alive until submissions finish. `Renderer::begin` uploads camera constants once and resets binding state; `draw` binds changed mesh/instance/material state and emits an indexed instance draw. This is a usable small rendering core, not a complete game engine: texture materials, shadows, skinning, asset import, audio, robust device-loss recovery and arbitrary dynamic resolution are not implemented.

## Optimizations retained

- Native 3840x2160 SurfaceView/swapchain, without dynamic resolution or geometry LOD reductions.
- Two frames in flight with separate mapped instance buffers and command buffers; per-image presentation semaphores.
- One opaque render pass, depth testing, back-face culling and front-to-back submission.
- D16 transient depth with CLEAR/DONT_CARE load/store, backed by the driver's DEVICE_LOCAL | LAZILY_ALLOCATED memory type. Colour is stored for presentation.
- Persistently mapped device-local, host-visible coherent memory on this UMA GPU. Noncoherent memory remains a supported fallback with explicit flushes.
- Cached visibility bounds and distance keys: avoids repeating distance calculations and moving 80-byte instances during comparisons. No steady-state scene vector growth.
- Indexed shared geometry, 16-bit index buffers for these meshes, and instancing.
- Camera push constants once per pass; skip redundant pipeline and vertex/index buffer bindings.
- Four precompiled pipeline specializations; no runtime shader branching for material selection.
- CPU geometry copies released after upload where the example no longer needs them.
- Cortex-A55 compilation option. A standalone speedup from this flag was not demonstrated.

## Physical device findings

Verified live on 2026-10-22:

- Android 11 V620, 32-bit application ABI; four Cortex-A55 CPUs (part 0xd05), NEON/VFPv4. Sampled CPU frequency 1.3 GHz.
- Mali-G31, Vulkan 1.1.131, driverVersion 109051904.
- Memory types: coherent shared device memory, CPU-cached noncoherent shared device memory, and lazy device-local memory. The transient depth attachment actually selects the lazy type.
- ASTC and ETC2 texture compression advertised. These are useful for a future texture system; no texture support is claimed here.
- No multiDrawIndirect, Vulkan timestamp bits, or pipeline-statistics-query support reported. Do not enable unsupported GPU-driven paths or timestamp queries.
- GPU clock/policy/counter sysfs nodes deny access to the debugging shell. No power policy, frequency, root, firmware or network changes were made.
- SurfaceFlinger previously confirmed 4K buffers with hardware composition and compression. Android screenshots are 1080p captures; their dimensions do not describe the render resolution or independently establish physical panel pixel mapping.

## Measurements and interpretation

All heavy comparisons used the same scene time 3.0, 3840x2160, 8192 total added objects, 5165 visible instances and 68318 submitted triangles. The normal scene contains terrain, trees, pillars, animated water and twelve gems. No reduction of geometry or resolution was used to improve the stress result.

Earlier matched measurements: optimized Vulkan about 55 fps versus GLES about 49 fps after applying the same CPU sorting improvement. GLES was then removed as requested. Historical captures remain in evidence; the current APK cannot run that backend.

Normal island and 2048-object versions reached about 60 fps. The 8192-object case remains around 53-55 fps and misses some display refreshes; it is not a locked 60-fps result. SurfaceFlinger presentation timestamps are the independent output metric; application FPS and CPU timers are separate diagnostics. Tests are short on-device samples, not controlled thermal/clock experiments or guarantees for arbitrary game content.

The original sequential Vulkan loop achieved roughly 30 fps. Two frame slots raised that to roughly 43 fps. Cached sort keys reduced cull/sort/upload time from about 11.7 ms to 3-4 ms and brought the heavy scene to roughly 53-55 fps. CPU recording is below 1 ms. Fence/acquire/present waits include GPU and display scheduling and must not be described as exact GPU execution times.

Experiments not retained: cached noncoherent memory did not show a clear gain; three frames in flight shifted waits without improving throughput; preserving colour-image layouts did not improve the tested fixed or moving scenes. Both the larger frame ring and extra layout-tracking path were removed. Exact remaining shader-versus-bandwidth cost has not been established because GPU timestamps/counters are unavailable through the tested path.

Latest artifacts: `evidence/vulkan-release-*-present.txt`, corresponding `*-log.txt`, and `evidence/vulkan-hardware-results.json`. Earlier stages and rejected experiments retain separately named evidence. Full system dumps contain local diagnostic state and should be reviewed before publication.

## Reproduce a fixed workload

```powershell
adb -s 192.168.50.35:5555 shell am force-stop dev.tcl.rustengine
adb -s 192.168.50.35:5555 shell am start -n dev.tcl.rustengine/.VulkanActivity --ei objects 8192 --ez tour true --ef benchmarkTime 3
adb -s 192.168.50.35:5555 logcat -v threadtime Rust3D:I '*:S'
adb -s 192.168.50.35:5555 shell 'dumpsys SurfaceFlinger --latency "SurfaceView - dev.tcl.rustengine/dev.tcl.rustengine.VulkanActivity#0"'
```

Omit benchmarkTime for the moving tour; omit tour and objects for interactive play. `--ei frames 600` makes a bounded test. Force-stop before changing benchmark extras because the launcher uses a single activity task. Pause/resume recreates the scene and GPU resources; game-state persistence is not implemented. An unexpected surface/device error logs VULKAN FAIL and requires reopening the activity.

Validation: five Rust host tests, Android release build, SPIR-V validation, on-device image inspection, presentation timing and Home/resume exercise. Validation layers were not enabled on the TV.

Hardware guidance used: [Khronos tile-based rendering best practices](https://docs.vulkan.org/guide/latest/tile_based_rendering_best_practices.html), [Arm Mali-G31 counter reference](https://arm-software.github.io/libGPUCounters/mali-g31_counter_reference.html), and [Arm Vulkan memory limits](https://github.com/ARM-software/vulkan_best_practice_for_mobile_developers/blob/master/docs/memory_limits.md).
