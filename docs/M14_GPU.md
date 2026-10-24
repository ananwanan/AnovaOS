<!-- Define bounded VirGL rendering, compositor admission, coherent DMA ownership and CPU fallback. -->
# M14.5: bounded offscreen GPU drawing

This profile implements an optional modern virtio-gpu control queue and a
compositor-only offscreen clear/readback operation. It is separate from the
accepted [software desktop](M14_COMPOSITOR.md) and [GUI SDK](M14_RUNTIME.md).
Actual hardware and regression results belong in [validation/M14.5](validation/M14.5.md).

## Rendering and presentation

The guest independently implements the modern PCI transport and the required
VirtIO/VirGL wire subset in Rust. It negotiates `VERSION_1` and `VIRGL`, checks
capset 2 and the BGRA render-target format, creates a context and 2D render-target
resource, attaches four owned backing pages, and submits a VirGL surface,
framebuffer binding and `CLEAR` through `SUBMIT_3D`. A fenced
`TRANSFER_FROM_HOST_3D` copies the host-rendered result into guest DMA storage.
No `TRANSFER_TO_HOST_2D` or precomputed reference image supplies the result.

Before readback every backing pixel contains distinct poison. Both the driver
and compositor compare the returned RGB pixels with an independent CPU color
reference. The driver checks opaque alpha as well. UNORM channel values are
encoded with integer arithmetic so the kernel never executes floating-point
instructions. Host tests compare every 8-bit channel against an independent
IEEE-754 calculation.

The secondary GPU renders offscreen. Existing checked standard-VGA/Limine
scanout remains the recovery display: the compositor presents readback pixels
using the existing copied `PRESENT` operation. This profile demonstrates GPU
render-target clearing and readback, followed by CPU display copying. It does
not accelerate ordinary window composition, demonstrate shader draws or imply
an OpenGL/Vulkan API, guest Mesa, shared scanout buffers, page flips or a speedup.

## Admission and ownership

The experimental `SYS_GUI` transport adds `GPU_CLEAR = 12`. Only the authenticated
bound compositor can call it. This does not change frozen native syscall v1 or
the GUI library C ABI. Its padding-free 24-byte `GpuClear` request contains:

| Field | Contract |
| --- | --- |
| `pixels: u64` | Exclusively writable output of `width * height` packed RGB `u32` pixels |
| `width, height: u32` | Each in `1..=64`; tightly packed rows |
| `rgb: u32` | `0x00RRGGBB`, with high bits rejected |
| `reserved: u32` | Must be zero |

The kernel checks the exact request size, ownership, geometry, format and full
writable output span before device access. It copies the request, renders into
kernel-owned DMA, stages initialized pixels privately and uses checked usercopy
after completion. No client-supplied pointer, context, resource ID or command stream
reaches the host. A usercopy failure returns an error without retaining any user address.
No allocation or address-space reference remains suspended across scheduling.

One BSP at IF=0 serializes all GPU work. The device receives a distinct bounded
supervisor UC/NX PCI window, one WB-coherent DMA32 queue page and four backing
pages. The eight-entry split queue has one outstanding two-descriptor request.
Each command carries a unique fence. Completion checks the used index, descriptor
ID, returned length, expected response, fence flags/identity, context and padding.
Memory fences order request publication and completed device reads. Configuration
generation brackets capability-count reads. IRQs remain masked.

Every call owns a temporary device session; acknowledged reset precedes release
of queues and backing pages. Error paths use the same teardown. An unacknowledged
reset retains every exposed DMA page, disables bus mastering and permanently
disables subsequent GPU calls. That quarantine path is a containment policy;
successful-reset tests do not prove recovery from a device that refuses reset.

## Recovery and gates

`gpu` enables the production operation; `gpu-test` additionally enables the
server-only scalar `GPU_TEST_TIMEOUT = 13` and an invalid-capset device probe.
The timeout operation publishes a real control request while dropping its notification,
requires the calibrated watchdog to expire, then resets the actual device.
Production kernels reject the injection operation. Both PM time and iteration
bounds keep a broken device from blocking indefinitely.

The explicit compositor diagnostics `compositor gpu` and `compositor gpu-fail`
exercise the API, compare readbacks and select CPU fallback in user space.
The ordinary desktop retains its existing software renderer.

```sh
cargo xtask gpu-test           # Requires an actual hardware GL backend
cargo xtask gpu-fallback-test  # Headless 2D-only and absent-device CPU paths
```

The full gate checks this exact QEMU device's capset renderer string, rejects
software renderers case-insensitively and requires a recognized hardware identity.
A separate host GL probe does not establish this device's backend. Artifacts in
`build/gpu/` include commands, input hashes, package versions, serial/QMP logs,
render/readback timings, independent whole-screen comparisons and exit results.
The headless gate is suitable for ordinary CI. It never substitutes for hardware
acceptance, and the full gate fails when hardware evidence is missing.

For WSLg, the runner selects Gallium `d3d12` for its QEMU child when `/dev/dxg`
exists and no driver was explicitly selected. `GDK_BACKEND`, `GALLIUM_DRIVER`
and `ANOVA_GPU_DISPLAY` can select another supported environment. Software
backends remain insufficient for full acceptance even when the protocol works.

## Primary protocol references

- [VirtIO 1.2](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html),
  GPU control queue, 3D commands, fences and modern PCI transport.
- [QEMU GPU backend distinctions](https://www.qemu.org/docs/master/system/devices/virtio/virtio-gpu.html)
  and [QEMU 8.2.2 VirGL transport](https://gitlab.com/qemu-project/qemu/-/blob/v8.2.2/hw/display/virtio-gpu-virgl.c).
- VirGL wire references:
  [protocol](https://gitlab.freedesktop.org/virgl/virglrenderer/-/blob/1.0.1/src/virgl_protocol.h),
  [capability layouts](https://gitlab.freedesktop.org/virgl/virglrenderer/-/blob/1.0.1/src/virgl_hw.h)
  and [host clear/readback implementation](https://gitlab.freedesktop.org/virgl/virglrenderer/-/blob/1.0.1/src/vrend_renderer.c).

These references describe the independently encoded protocol. No upstream
renderer, Linux GPU driver or Mesa runtime is vendored into the guest.
