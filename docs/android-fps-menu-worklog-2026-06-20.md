# RustAdmin Android FPS and menu worklog, 2026-06-20

## Репозитории

- Основной репозиторий: `/home/w0w/rustadmin-fps-diag`
- Внешний proto-репозиторий: `/home/w0w/hbb_common`
- Целевой upstream: `RustAdministrator/rustadmin`
- Fork для PR: `woffko/rustadmin`

## Исходная проблема

На Android при подключении к хосту с 2K-разрешением, например `2560x1440`, FPS держался около `15-16` независимо от выбранного кодека: VP9, AV1, H.264 или H.265.

Первичная гипотеза: ограничение не в выборе кодека, а в Android decode/render pipeline:

- H.264/H.265 на Android шли через MediaCodec, но результат забирался как CPU-доступный YUV/I420 buffer.
- После этого выполнялась CPU-конвертация YUV/I420 в RGBA через `I420ToARGB` / `I420ToABGR`.
- Затем RGBA buffer передавался во Flutter soft render path.
- `decode_fps` фактически включал не только декодер, но и часть render/callback pipeline.
- Adaptive FPS мог ограничивать удаленный FPS через `min_decode_fps`.

## Что было сделано по Android video pipeline

### Диагностика decode/render pipeline

Добавлена Android-специфичная диагностика, чтобы разделить:

- активный codec path;
- render path;
- resolution;
- video queue length;
- measured decode FPS;
- auto FPS;
- direct/relay mode;
- MediaCodec input/output timing;
- YUV/RGBA conversion timing;
- total frame handling timing;
- Flutter handoff timing;
- RGBA buffer size;
- факт realloc RGBA buffer;
- output buffer size;
- MediaFormat/stride/crop/slice-height/color-format details.

Основные файлы:

- `libs/scrap/src/common/mod.rs`
- `libs/scrap/src/common/codec.rs`
- `libs/scrap/src/common/mediacodec.rs`
- `src/client.rs`
- `src/client/io_loop.rs`
- `src/flutter.rs`
- `flutter/lib/models/model.dart`
- `flutter/lib/common/widgets/overlay.dart`

### Quality Monitor вместо logcat

Часть данных перенесена в Quality Monitor, чтобы не полагаться только на `adb logcat`.

На Android viewer в Quality Monitor теперь выводятся client-side поля:

- `Path`
- `Render`
- `Res`
- `Queue`
- `DecFPS`
- `AutoFPS`
- `Mode`
- `Direct`
- `MC in`
- `MC out`
- `YUV->RGBA`
- `MC dec`
- `Frame`
- `Flutter`
- `Total`
- `RGBA`
- `Realloc`
- `Out buf`
- `Format`

Чтобы снизить накладные расходы, частые pipeline-поля во Flutter throttled примерно до одного обновления в секунду.

### Host-side данные в Quality Monitor

Добавлена передача host-side diagnostic snapshot через `TestDelay`, чтобы видеть не только Android viewer, но и удаленный host:

- `HostFPS`
- `HostCodec`
- `HostQoS`
- `HostWait`

Изменения:

- `/home/w0w/hbb_common/protos/message.proto`
- `src/server/video_service.rs`
- `src/server/connection.rs`
- `src/ui_session_interface.rs`
- `src/client/helper.rs`
- `src/flutter.rs`
- `flutter/lib/models/model.dart`
- `flutter/lib/common/widgets/overlay.dart`

Важно: эти строки появятся в Quality Monitor только если удаленный host тоже собран с обновленным proto/server-кодом. Если Android APK обновлен, а Windows/Linux host старый, поля `Host...` будут пустыми.

### Вывод по скриншотам

По скриншотам Android viewer показывал примерно:

- `FPS 15`
- `Codec H264`
- `Path hwram_h264`
- `Render rgba_soft_render`
- `Res 2560x1440`
- `Queue 0`
- `DecFPS 72-94`
- `AutoFPS 30`
- `Mode adaptive`
- `Direct true`
- `Frame 9-17 ms`
- `Flutter около 0.1 ms или меньше`

Это говорит, что в этом конкретном замере:

- очередь на Android viewer не забита (`Queue 0`);
- Android decode/render sample не выглядит как причина 15 FPS (`DecFPS` сильно выше 15);
- adaptive client cap выставлен на 30, а не на 15;
- соединение direct, задержка низкая;
- нужны host-side данные, чтобы понять, ограничивает ли FPS capture/encode/send сторона хоста.

## Исправления MediaCodec

Проверен и поправлен `libs/scrap/src/common/mediacodec.rs`.

Сделано:

- добавлены timing-поля для MediaCodec input/output и conversion;
- добавлена диагностика MediaFormat;
- поправлена обработка stride/crop/slice-height;
- добавлен warning/fallback для unexpected color format;
- убраны лишние realloc RGBA buffer, где это возможно;
- исправлена ошибка duplicate `ImageFormat::ARGB` match arm: второй arm должен быть `ImageFormat::ABGR`;
- сохранен fallback на существующий RGBA soft-render path.

Текущий pipeline для Android H.264/H.265 остается byte-buffer based:

```text
MediaCodec decode -> YUV/I420 output buffer -> CPU YUV/RGBA conversion -> RGBA Vec<u8> -> Flutter soft render
```

Желаемый будущий pipeline описан отдельно:

```text
MediaCodec decode -> Surface/SurfaceTexture -> Flutter texture
```

Документ:

- `docs/android-video-pipeline.md`

## Texture path

Полный Android SurfaceTexture/Flutter texture path не был реализован в этой итерации, потому что это отдельная интеграция с жизненным циклом Flutter texture registration, Surface/SurfaceTexture и MediaCodec surface output.

Оставлены требования к безопасному будущему варианту:

- если texture init failed, fallback to RGBA soft render;
- если Flutter texture registration failed, fallback to RGBA soft render;
- если MediaCodec Surface output failed, fallback to byte-buffer decode;
- если texture delivery failed at runtime, fallback to RGBA soft render;
- VP8/VP9/AV1 software decode paths не трогать;
- desktop platforms не ломать.

## Исправления Quality Monitor и меню Android

### Пропавший пункт Quality Monitor

Были возвращены пункты меню, которые ранее пропали после изменений в mobile toolbar/menu.

Затронутые файлы:

- `flutter/lib/mobile/pages/remote_page.dart`
- `flutter/lib/mobile/pages/view_camera_page.dart`
- `flutter/lib/common/widgets/setting_widgets.dart`
- `src/lang/ru.rs`

### Clipboard heading

После возврата пунктов меню пропал заголовок секции clipboard. Он возвращен в оба мобильных меню:

- `remote_page.dart`: `Clipboard direction`
- `view_camera_page.dart`: `Clipboard direction`

### Custom quality FPS mode dropdown

На Android dropdown для выбора режима custom quality:

- `Adaptive FPS cap`
- `Fixed FPS`

рисовался вне меню/overlay, из-за чего выбрать пункт было почти невозможно.

Исправление: на mobile UI dropdown заменен на inline `RadioListTile` внутри самого меню. Desktop UI оставлен с dropdown.

Файл:

- `flutter/lib/common/widgets/setting_widgets.dart`

## Исправления сборки Android

Сборка Android была доведена до успешного release APK.

Сделано:

- поправлен Gradle proto path в `flutter/android/app/build.gradle`;
- обновлены/pinned Flutter зависимости в `flutter/pubspec.yaml` и `flutter/pubspec.lock`;
- внесены правки совместимости с Flutter/Dart:
  - `DialogTheme`;
  - deprecated `withOpacity`;
  - удаление/замена несовместимого `selectAllOnFocus`;
  - упрощение controller logic в desktop remote toolbar;
- поправлены Android wake lock lifetime и clipboard compile issues;
- обновлен `flutter/ndk_arm64.sh`.

## Проверки

Выполнялись:

```bash
cargo ndk check --features flutter,hwcodec,mediacodec
cargo ndk build --release --features flutter,hwcodec,mediacodec
/home/w0w/flutter/bin/dart format ...
/home/w0w/flutter/bin/flutter build apk --target-platform android-arm64 --release --build-name 2.0.2 --build-number 2202
/home/w0w/android-sdk/build-tools/34.0.0/apksigner verify --verbose ...
/home/w0w/android-sdk/build-tools/34.0.0/aapt dump badging ...
unzip -l ... lib/arm64-v8a/*
git diff --check
```

Результат:

- Android APK собран успешно.
- APK signature v1/v2 valid.
- Package: `io.github.rustadministrator.rustadmin`
- Version name: `2.0.2`
- Version code: `2202`
- Native ABI: `arm64-v8a`
- `git diff --check` чистый.
- Временные signing-файлы после сборки удалены из рабочей копии.

## Последний APK

Файл:

```text
/home/w0w/rustadmin-fps-diag/flutter/build/app/outputs/flutter-apk/rustadmin-2.0.2-v2202-arm64-v8a-menu-hostdiag-release-20260620.apk
```

SHA256:

```text
697ce4d60c07113dfe62f0af5ec76b3eeb7d289bfb465e35955d23359098373f
```

## Текущий статус рабочих деревьев

Основной репозиторий содержит изменения в Android/Flutter UI, diagnostics, MediaCodec, server host diagnostics и документации.

Внешний `/home/w0w/hbb_common` содержит изменение:

```text
M protos/message.proto
```

Добавленные поля `TestDelay`:

```proto
string host_video_fps = 5;
string host_video_codec = 6;
string host_video_qos = 7;
string host_video_wait = 8;
```

## Что важно сделать дальше

1. Собрать и установить обновленный host binary на удаленный хост.
2. Проверить Quality Monitor после обновления host:
   - `HostFPS`
   - `HostCodec`
   - `HostQoS`
   - `HostWait`
3. Повторить тесты на 2K:
   - adaptive FPS;
   - fixed FPS 30;
   - H.264;
   - H.265;
   - VP9;
   - AV1;
   - direct;
   - relay.
4. Если `HostFPS/HostWait/HostQoS` покажут ограничение на capture/encode/send стороне, дальше оптимизировать host path.
5. Если host будет отдавать 30 FPS стабильно, а Android снова покажет 15 FPS при низком `DecFPS`, возвращаться к Android texture path.

## Краткий технический вывод

На текущих скриншотах Android viewer не выглядит главным ограничителем: `DecFPS` выше целевого, `Queue 0`, `AutoFPS 30`, `Direct true`.

Вероятнее всего, текущий 15 FPS нужно подтверждать host-side диагностикой. Без обновленного host эти поля в Quality Monitor не появятся, потому что старый host не отправляет новые поля `TestDelay`.
