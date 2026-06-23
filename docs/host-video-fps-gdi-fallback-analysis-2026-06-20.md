# Host Video FPS / GDI Fallback Analysis

Date: 2026-06-20
Context: RustAdmin/RustDesk-derived Android client FPS diagnostics, host video service logs from 2026-06-19 20:39-22:29 +03:00.

This is a handoff note for another maintainer or Codex session. No source-code changes are included in this note.

## Executive Summary

The logs point to a host-side capture bottleneck, not an Android decode bottleneck.

The host successfully negotiates and creates hardware encoders:

- H264: `h264_nvenc`, `hardware=true`
- H265/HEVC: `hevc_nvenc`, `hardware=true`

However, each session starts with DXGI capture and then quickly switches to GDI:

```text
gdi: false
No image, fall back to gdi
diag host fps: ... gdi=true ...
```

After this switch, the host-side diagnostics frequently report `valid_capture=75`, `encode_calls=75`, and `sent_batches=75` over the roughly 5 second diagnostic interval. That is about 15 FPS, even when `target_fps=30.0`.

So the observed Android Quality Monitor value around 15 FPS is consistent with the host only capturing/encoding/sending about 15 FPS. The Android client is probably displaying what it receives.

## Evidence From The Log

The connection is direct LAN access:

```text
direct access from [::ffff:10.0.87.166]
peer_name=Xiaomi-24129PN74G, platform=Android, version=2.0.2
```

The host selects hardware video encoding:

```text
cfg=HWRAM(HwRamEncoderConfig { name: "h264_nvenc", ... })
hardware=true

cfg=HWRAM(HwRamEncoderConfig { name: "hevc_nvenc", ... })
hardware=true
```

The capture path falls back to GDI very early:

```text
diag video service capturer ready: ... gdi=false
No image, fall back to gdi
diag host fps: ... gdi=true ...
```

Representative host FPS lines:

```text
target_fps=30.0, gdi=true, valid_capture=75, encode_calls=75, sent_batches=75
target_fps=30.0, gdi=true, valid_capture=74, encode_calls=74, sent_batches=74
target_fps=30.0, gdi=true, valid_capture=75, encode_calls=75, sent_batches=75
```

Those samples are logged at about 5 second intervals, so `75 / 5 = 15 FPS`.

Network/send wait does not look like the primary limiter in the good H264/H265 runs:

```text
wait_avg_ms=0
wait_max_ms=0
empty_send_results=0
```

There are also virtual display errors:

```text
Failed to install driver: Driver inf file not found.
Failed to plug in virtual display: Failed to install driver.
```

That is likely a separate issue unless the test depends on virtual displays. It should be surfaced separately in diagnostics because it can confuse capture/display behavior.

## Likely Root Cause

The server currently appears to treat a few consecutive DXGI `WouldBlock` results as a reason to permanently switch to GDI.

On Windows Desktop Duplication, `WouldBlock`/timeout often means "there is no new desktop frame yet", not "DXGI is broken". This is common on a static desktop, especially just after session startup.

The relevant area to inspect is around the video service capture loop:

- `src/server/video_service.rs`
- `libs/scrap/src/common/dxgi.rs`
- `libs/scrap/src/dxgi/mod.rs`

The suspicious behavior is:

1. Start DXGI capture.
2. DXGI returns `WouldBlock` a few times before the first changed frame.
3. Server calls `set_gdi()`.
4. Session stays in GDI.
5. Host capture/encode/send rate becomes about 15 FPS under a 30 FPS target.

This explains why changing H264/H265/quality/FPS on the Android side does not fully solve the issue.

## Recommended Investigation / Fix Direction

Do not treat early DXGI `WouldBlock` as a permanent DXGI failure.

Possible approaches:

- Keep DXGI active on `WouldBlock`; only fallback to GDI on real DXGI errors.
- If an initial still image is needed, use GDI only as a temporary startup snapshot and then immediately return to DXGI.
- Add a longer grace window before fallback, and log the reason/count clearly.
- If falling back to GDI is necessary, expose the fallback reason in Quality Monitor.
- Consider a recovery path from GDI back to DXGI after a short interval or after desktop activity resumes.

Real DXGI errors should still fallback to GDI.

## Quality Monitor Metrics To Add Or Keep

The client Quality Monitor should show enough information to distinguish:

- host capture bottleneck
- host encode bottleneck
- network/send bottleneck
- Android decode bottleneck
- Android render bottleneck

### Host-Side Metrics

Recommended fields:

- `HostTargetFPS`: current server target FPS after QoS.
- `HostCaptureFPS`: valid captured frames per second.
- `HostEncodeFPS`: encoder calls per second.
- `HostSentFPS`: non-empty send batches per second.
- `HostCodec`: negotiated codec, for example `H264`, `H265`, `VP9`, `AV1`.
- `HostEncoder`: encoder implementation/name, for example `h264_nvenc`, `hevc_nvenc`, `vpx`, `aom`.
- `HostHW`: hardware encoder true/false.
- `HostCaptureBackend`: `DXGI`, `GDI`, `WinMag`, `Wayland`, `X11`, etc.
- `HostFallbackReason`: `none`, `dxgi_would_block_startup`, `dxgi_error`, `directx_disabled`, `privacy_mode`, etc.
- `HostBitrate`: active encoder bitrate.
- `HostQuality`: active quality ratio.
- `HostResolution`: capture width/height.
- `HostValidCapture`: count in sample window.
- `HostInvalidCapture`: count in sample window.
- `HostWouldBlock`: capture would-block count in sample window.
- `HostRepeatEncode`: repeated encodes from previous frame.
- `HostEmptySend`: encode/send attempts with no targets or no payload.
- `HostWaitAvgMs`: average wait for client frame fetch/ack.
- `HostWaitMaxMs`: maximum wait for client frame fetch/ack.
- `HostWaitTimeouts`: wait timeout count.
- `HostStaleDrops`: stale video frames dropped before send.
- `HostFirstFrameMs`: first frame capture-to-encode latency.
- `HostVirtualDisplayStatus`: driver installed/failed, plug-in result.

The most important fields for this bug are:

```text
HostTargetFPS
HostCaptureFPS
HostEncodeFPS
HostSentFPS
HostCaptureBackend
HostFallbackReason
HostWouldBlock
HostCodec
HostHW
HostWaitAvgMs
HostWaitMaxMs
```

### Android Client Metrics

Recommended Android-specific fields:

- `AndroidDecodeFPS`: decoded frames per second.
- `AndroidRenderFPS`: frames rendered to screen per second.
- `AndroidDroppedFrames`: render/drop count in the sample window.
- `AndroidCodecPath`: `MediaCodec`, `software`, `texture`, `RGBA`, etc.
- `AndroidDecoderName`: actual MediaCodec decoder name when available.
- `AndroidCodec`: H264/H265/VP9/AV1.
- `AndroidSurfaceMode`: Surface rendering vs ByteBuffer/RGBA copy path.
- `AndroidDecodeQueueMs`: input queue time.
- `AndroidOutputDequeueMs`: output dequeue time.
- `AndroidYuvToRgbaMs`: conversion cost when using ByteBuffer/RGBA path.
- `AndroidHandleFrameMs`: total frame handling time.
- `AndroidRenderMs`: UI/render submission time if measurable.
- `AndroidInputBytes`: compressed payload bytes per frame/sample.
- `AndroidOutputBytes`: decoded output buffer size.
- `AndroidMediaFormat`: width, height, stride, slice-height, crop rect, color-format.
- `AndroidRgbaReallocated`: whether RGBA buffers are reallocating.
- `AndroidRequestedFPS`: requested FPS from UI.
- `AndroidFPSMode`: adaptive cap vs fixed FPS.
- `AndroidSupportedDecoding`: client advertised H264/H265/VP9/AV1 support and preference.
- `AndroidThermalState`: if accessible, thermal throttling state.
- `AndroidCpuLoad`: if accessible, process or system CPU load.

For this issue, Android should show host metrics next to Android metrics. Otherwise a host-side 15 FPS cap looks like a decoder problem.

## Current Interpretation

The H264/H265 hardware pipeline is available and generally healthy. The limiting factor in the supplied logs is the host capture backend switching to GDI after early DXGI no-frame events. The observed 15 FPS on Android matches host `valid_capture`/`encode_calls`/`sent_batches`, so the next fix should focus on Windows host capture fallback behavior and on exposing host capture backend/FPS in the client Quality Monitor.
