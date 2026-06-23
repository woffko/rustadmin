# Upstream Pickup Notes: Video Diagnostics And Capture Backend

Date: 2026-06-23

This branch is intended as a stable pickup point for the RustAdmin video diagnostics,
Android Quality Monitor, and Windows capture-backend work.

## RustAdmin Branch

- Fork: `https://github.com/woffko/rustadmin`
- Branch: `pickup/video-diagnostics-capture-backend`
- Base in the fork: `5e72cedef25f44807d36a15145b48324a8ef0837`
- Main source sync commit: `5e72cedef` (`Sync RustAdmin source state`)

The fork `main` already contains the same RustAdmin source state. This pickup branch
adds only these notes on top, so maintainers can compare or cherry-pick from a stable
branch name without tracking the moving fork main.

## Included RustAdmin Work

- Android/mobile Quality Monitor restores the missing session menu options and adds a
  `Debug mode` toggle for full diagnostics.
- Quality Monitor now reports client and host full versions when available.
- Android About now reads the Rust full version string, so it can show revision text.
- Windows host video diagnostics include capture FPS, codec, QoS, wait, backend, and
  fallback reason.
- Windows capture backend can be selected at runtime: `Auto`, `DXGI`, `WGC`, `WinMag`,
  or `GDI`.
- The Capture menu is capability-gated by `platform_additions.support_capture_backend`
  instead of platform text alone, so old or non-Windows hosts do not show a dead menu.
- WGC was added to the Windows capture fallback chain.
- Host-side pacing/stale-frame handling was tightened for smoother recovery from stalls.

## Workflow Scope Note

The local PAT used for this fork does not have GitHub `workflow` scope. Because of that,
the fork branch intentionally does not update active files under `.github/workflows/*`.
Workflow files copied into `.github/workflows-disabled/` are ordinary source files and
are included for reference.

## hbb_common Dependency

The RustAdmin changes depend on matching `hbb_common` protocol/config changes. In this
workspace those changes are in local commit:

```text
b7e102f9d74952cf260d049d213cdc8077a90acd Add capture backend session option
```

That commit was not pushed to `ssh4net/hbb_common`; this branch does not require any
write access to `ssh4net`. Upstream can apply the equivalent patch to `hbb_common`:

```diff
diff --git a/protos/message.proto b/protos/message.proto
--- a/protos/message.proto
+++ b/protos/message.proto
@@
 message OptionMessage {
   enum BoolOption {
     NotSet = 0;
     No = 1;
     Yes = 2;
   }
+  enum CaptureBackend {
+    CaptureBackendNotSet = 0;
+    CaptureBackendAuto = 1;
+    CaptureBackendDxgi = 2;
+    CaptureBackendWgc = 3;
+    CaptureBackendWinMag = 4;
+    CaptureBackendGdi = 5;
+  }
   ImageQuality image_quality = 1;
   BoolOption lock_after_session_end = 2;
   BoolOption show_remote_cursor = 3;
@@
   BoolOption disable_camera = 17;
   BoolOption terminal_persistent = 18;
   BoolOption show_my_cursor = 19;
+  CaptureBackend capture_backend = 20;
 }
 
 message TestDelay {
   int64 time = 1;
   bool from_client = 2;
   uint32 last_delay = 3;
   uint32 target_bitrate = 4;
+  string host_video_fps = 5;
+  string host_video_codec = 6;
+  string host_video_qos = 7;
+  string host_video_wait = 8;
+  string host_video_backend = 9;
+  string host_video_fallback = 10;
 }
diff --git a/src/config.rs b/src/config.rs
--- a/src/config.rs
+++ b/src/config.rs
@@
     pub const OPTION_CUSTOM_FPS: &str = "custom-fps";
     pub const OPTION_CUSTOM_FPS_MODE: &str = "custom-fps-mode";
     pub const OPTION_CODEC_PREFERENCE: &str = "codec-preference";
+    pub const OPTION_CAPTURE_BACKEND: &str = "capture-backend";
     pub const OPTION_SYNC_INIT_CLIPBOARD: &str = "sync-init-clipboard";
```

After applying the `hbb_common` patch, regenerate any protobuf bindings according to the
normal project workflow if the target tree does not generate them during the build.

## Verified Local Builds

- Android arm64 release APK: `RustAdmin_Android_Release_2.0.2.006.apk`
- Android arm64 release ZIP: `RustAdmin_Android_Release_2.0.2.006.zip`
- Windows release ZIP: `RustAdmin_Release_2.0.2.006.zip`

See `in_progress.md` for hashes, sizes, and verification notes.
