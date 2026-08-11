# Smilie on iOS + Android

Smilie uses Tauri v2's mobile target. The Rust crate cross-compiles
cleanly; this doc covers the per-platform setup you need to do once.

## Status (v1)

| Feature | iOS | Android |
| --- | --- | --- |
| Library + page viewer | ✅ | ✅ |
| Settings | ✅ | ✅ |
| Bottom-sheet sidebar + Pages FAB | ✅ | ✅ |
| Native file-picker ingest | ✅ | ✅ |
| Share-sheet ingest (Files / Mail → Smilie) | wired, requires Info.plist | wired, requires intent-filter |
| PDF page rendering on-device | ❌ ingest from desktop for now | ❌ ingest from desktop for now |
| Inference runtime | llama.cpp (GGUF) | llama.cpp (GGUF) |
| Background processing | spec'd (see below), not wired | spec'd, not wired |

The pipeline gate is `render_pages`: on mobile that stage returns a
clear error pointing the user to ingest on desktop. Everything that
follows (OCR, reasoning, generation, viewing) works once the page
images exist — so a future native-renderer follow-up unlocks full
mobile end-to-end without further plumbing.

## Prerequisites

- **iOS** — macOS 14+, Xcode 15+, Apple Developer account
- **Android** — Android Studio Hedgehog+, NDK 26+, JDK 17, ANDROID_HOME +
  NDK_HOME exported

## One-time init

```bash
# iOS project scaffold (writes src-tauri/gen/apple)
npm run tauri:ios:init

# Android project scaffold (writes src-tauri/gen/android)
npm run tauri:android:init
```

Tauri writes platform projects under `src-tauri/gen/{apple,android}`,
which are gitignored.

## Dev run

```bash
npm run tauri:ios:dev       # boots simulator / attached device
npm run tauri:android:dev   # boots emulator / attached device
```

## Production builds

```bash
npm run tauri:ios:build      # → .ipa
npm run tauri:android:build  # → .apk + .aab
```

## How file intake works on mobile

Native desktop drag-drop does not exist on mobile. Smilie's `Dropzone`
component detects this and falls back to the Tauri file picker, which on
iOS/Android opens the system document browser.

For **share-sheet intake** (user picks Smilie in the "Share" menu from
Files, Mail, etc.), we use `tauri-plugin-deep-link`. You must register
the `application/pdf` UTI / intent filter in the generated platform
projects:

### iOS (`src-tauri/gen/apple/Smilie_iOS/Info.plist`)

```xml
<key>CFBundleDocumentTypes</key>
<array>
  <dict>
    <key>CFBundleTypeName</key><string>PDF</string>
    <key>LSHandlerRank</key><string>Alternate</string>
    <key>LSItemContentTypes</key>
    <array><string>com.adobe.pdf</string></array>
  </dict>
</array>
```

### Android (`src-tauri/gen/android/app/src/main/AndroidManifest.xml`)

Inside the main `<activity>` block:

```xml
<intent-filter>
  <action android:name="android.intent.action.VIEW" />
  <action android:name="android.intent.action.SEND" />
  <category android:name="android.intent.category.DEFAULT" />
  <category android:name="android.intent.category.BROWSABLE" />
  <data android:mimeType="application/pdf" />
</intent-filter>
```

## Runtime on mobile

Mobile uses the `llama.cpp` runtime via `llama-cpp-2` Rust bindings (no
python sidecar, no subprocess). Weights go into the mobile bundle's
`assets/models/` directory (small quantized GGUF files only — expect
1–3GB ceiling on phones).

Default mobile profile:

- **ocr**: `ocr_llama_mobile` (GLM-OCR 0.9B GGUF + mmproj)
- **reasoning + generator**: `reasoning_llama_mobile` (Gemma 4 E4B GGUF)

Edit `assets/models/manifest.json` to change profiles.

## Responsive UI

The Svelte frontend uses a single responsive layout — no mobile-specific
components. Sidebar switches to a bottom sheet below 720px, top bar
compacts, dropzone becomes tap-to-pick.

## Background processing (post-MVP, scoped)

Pipeline runs are long (multi-page OCR + reasoning + generation can take
minutes). On desktop, `tokio` keeps running while the user works in
other apps. Mobile OSes suspend backgrounded apps aggressively, which
will pause the pipeline mid-stage and surface as a stuck job. The fix
is platform-specific lifecycle hooks; this section is the spec for that
follow-up work.

### Goals

- A pipeline run kicked off in the foreground continues to completion
  while the user backgrounds the app for **at least 10 minutes** on
  iOS and indefinitely on Android.
- App restart resumes any in-flight job from the last completed stage
  (already true via `JobQueue::resume_on_startup`).
- User sees a system notification when a paper finishes (or fails)
  while the app is backgrounded.

### iOS

iOS gives short, app-initiated extensions, plus opt-in scheduled
background runs:

- **`UIApplication.beginBackgroundTask` / `BGTaskScheduler`** — when
  the app moves to background, request a finite-time extension
  (~30s typical, up to a few minutes) so the current pipeline stage
  can finish.
- **`BGProcessingTaskRequest`** — schedule longer (≥10 min) tasks
  when the user has plugged in / app is idle. Useful for the OCR
  stage, which is the heaviest step.
- **`UNUserNotificationCenter`** — local notification on
  pipeline:complete / pipeline:failed events.
- Plumbing: spawn this as a small Swift extension in
  `src-tauri/gen/apple/Smilie_iOS/`, exposed to Rust through a
  `tauri::ipc::Invoke` command (similar to how the deep-link plugin
  is wired). The pipeline stays in Rust; only lifecycle calls live
  in Swift.

### Android

Android is more permissive but requires foreground services for
long-running work:

- **`ForegroundService` with `dataSync` type** — show a persistent
  notification ("Smilie is generating learning pages") and run the
  pipeline inside the service. This is the most reliable option;
  WorkManager is too restrictive for streaming inference.
- **`WorkManager`** — fallback for shorter runs / scheduled retry.
- Plumbing: the service lives in
  `src-tauri/gen/android/app/src/main/java/...` as a Kotlin class.
  Rust pipeline runs inside the service's process; the service
  binds to it on app launch.

### Plugin vs. direct platform calls

Two implementation paths:

1. **Build `tauri-plugin-background`** — clean abstraction, contributes
   back to the ecosystem. Probably 1-2 weeks of work to get right
   (Swift + Kotlin + Rust + capability config + docs).
2. **Direct platform calls** — write the iOS/Android lifecycle code
   inline in this repo, expose a small Rust API. Faster (~3 days)
   but locks the work to Smilie.

**Recommendation:** start with #2 (direct), extract to a plugin if a
second app needs the same behavior. The Tauri team has flagged
"first-party background plugin" as a likely 2027 milestone, so we
might also be able to drop direct code in favor of the official
plugin later.

### Acceptance criteria for the follow-up task

1. Drop a 10-page paper, switch to another app for 5 min, return —
   pipeline has progressed (not paused at the same stage).
2. Pipeline failure during background → local notification with the
   stage name.
3. Pipeline complete during background → local notification with the
   paper title.
4. Force-killing the app via the multitasker still leaves the job
   resumable (no different from desktop kill behavior).
