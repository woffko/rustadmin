# Android video pipeline notes

Current H.264/H.265 Android viewer path:

1. MediaCodec is configured for byte-buffer output.
2. The decoder dequeues a CPU-accessible YUV/I420 output buffer.
3. The client converts I420 to ARGB/ABGR with libyuv.
4. The RGBA buffer is handed to Flutter through the soft-render path.

This path is safe and remains the mandatory fallback, but it can be CPU-copy bound at
2K resolutions because every decoded frame is copied and color-converted before Flutter
can render it.

Preferred future path:

1. Create an Android `SurfaceTexture` or equivalent external texture target.
2. Register that texture with Flutter and keep a stable texture ID per display/session.
3. Configure MediaCodec with a `Surface` output for H.264/H.265 when texture render is enabled.
4. On each decoded frame, release the MediaCodec output buffer with render enabled and notify
   Flutter that the texture has a new frame.
5. Preserve Quality Monitor codec reporting from the received `VideoFrame` codec metadata.

Required fallback points:

- SurfaceTexture creation failed -> byte-buffer RGBA soft render.
- Flutter texture registration failed -> byte-buffer RGBA soft render.
- MediaCodec configure with Surface failed -> byte-buffer MediaCodec decode.
- MediaCodec runtime Surface output failed -> reset decoder and return to byte-buffer RGBA soft render.
- Texture frame notification failed -> mark texture path unavailable for the session and return to RGBA soft render.

The diagnostic logs added around the current pipeline are intended to prove whether a device is
limited by MediaCodec dequeue, YUV-to-RGBA conversion, Flutter handoff, queue pressure, or adaptive
FPS control before replacing the render path.
