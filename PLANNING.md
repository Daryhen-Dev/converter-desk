# Video Downloader — Desktop (Rust + egui)

> Planning document for the desktop rewrite. Move this file into the new repo.
> A working, fully-tested Next.js reference implementation exists separately
> (project `converter`) — we reuse its **domain knowledge**, not its code.

---

## 1. Overview

A small, personal desktop application to download a video from a URL (e.g.
YouTube) at the highest available resolution, or extract audio-only as MP3.
It orchestrates the external `yt-dlp` and `ffmpeg` binaries and shows **live
download progress** (percent, speed, ETA, stage).

**Target platforms:** Arch Linux and Windows.
**Distribution goal:** a single native executable per OS — no runtime, no
"clone and run".

---

## 2. Goals & Non-Goals

### Goals
- Single URL -> download best-quality MP4, or audio-only MP3.
- Real-time progress feedback (never a blind "is it hung?" wait).
- Clean, layered architecture that a programmer understands at a glance.
- Native single-binary distribution for Arch + Windows.

### Non-Goals (MVP)
- No playlists / batch downloads (one URL = one video).
- No pause/resume, no download queue.
- No deployment/server, no auth, no multi-user.
- No format picker beyond the two modes.

---

## 3. Tech Stack

| Concern | Choice | Why |
|---|---|---|
| Language | Rust (2021 edition) | Native single binary, first-class on Arch, great process/stream handling |
| GUI | `egui` + `eframe` | Immediate-mode, beginner-friendly, cross-platform, built-in widgets (incl. progress bar) |
| Concurrency | `std::thread` + `std::sync::mpsc` | Keep the UI responsive; stream progress from a worker thread (no async runtime needed for MVP) |
| URL validation | `url` crate | Robust parsing/validation of user input |
| External tools | `yt-dlp`, `ffmpeg` | The actual download + mux/transcode engines |

---

## 4. External Dependencies: yt-dlp & ffmpeg

The app does **not** download by itself — it drives `yt-dlp` (and `ffmpeg` for
muxing/transcoding). This dependency does not disappear in a rewrite.

**Resolution strategy (carried over from the reference app):**
- Do **not** rely blindly on the ambient `PATH`.
- Resolve binaries via, in order: (1) a config value / env var
  (`YT_DLP_PATH`, `FFMPEG_PATH`), (2) a bundled binary next to the executable,
  (3) a plain `PATH` lookup as last resort.
- On startup, run a **preflight check** (`yt-dlp --version`, `ffmpeg -version`)
  and show an actionable message if either is missing.

**Bundling (decide during design):**
- Ship the binaries alongside the executable (simplest for the user; mind
  `ffmpeg` licensing — GPL/LGPL builds), OR
- Require them installed and document `winget` / `pacman` / `pip` install.

---

## 5. Architecture — Hexagonal (Ports & Adapters)

The whole point: **the domain does not know about egui, yt-dlp, or the OS.**
Dependencies point **inward**. Outer layers depend on inner layers, never the
reverse. This keeps the core pure and testable, and makes the structure
"scream" what the app does.

```
            +-------------------------------------------+
            |                    ui/                     |  presentation (egui)
            |   depends on ->  application               |
            +-------------------------------------------+
            |               application/                 |  use cases + PORTS (traits)
            |   depends on ->  domain                    |
            +-------------------------------------------+
            |                  domain/                   |  pure types, ZERO deps
            +-------------------------------------------+
                          ^
                          | implements ports
            +-------------------------------------------+
            |               infrastructure/              |  ADAPTERS (yt-dlp, ffmpeg, fs)
            +-------------------------------------------+
```

**The dependency rule:** `ui -> application -> domain`. `infrastructure`
implements the *ports* (traits) declared in `application`. `main.rs` is the
composition root: it wires concrete adapters into the use cases and launches
the UI. Nothing in `domain` or `application` imports `egui` or spawns a process.

### Folder / module structure

```
video-downloader-desktop/
├── Cargo.toml
├── README.md
├── PLANNING.md
├── assets/                         # (optional) bundled yt-dlp/ffmpeg, icons
└── src/
    ├── main.rs                     # composition root: wire adapters + launch eframe
    ├── app.rs                      # eframe::App impl: UI state + update loop
    │
    ├── domain/                     # PURE core — no I/O, no egui, no process
    │   ├── mod.rs
    │   ├── format.rs               # Format { VideoHighest, AudioMp3 }
    │   ├── media_url.rs            # validated MediaUrl value object
    │   └── job.rs                  # DownloadJob, JobStatus, Progress, Stage
    │
    ├── application/                # USE CASES + PORTS (traits)
    │   ├── mod.rs
    │   ├── ports.rs                # traits: MediaDownloader, BinaryProbe
    │   └── download_service.rs     # orchestrates a download, emits progress
    │
    ├── infrastructure/             # ADAPTERS to the outside world
    │   ├── mod.rs
    │   ├── ytdlp_downloader.rs     # implements MediaDownloader via yt-dlp process
    │   ├── binary_probe.rs         # implements BinaryProbe (yt-dlp/ffmpeg detection)
    │   ├── arg_builder.rs          # pure: builds the yt-dlp argument vector
    │   └── progress_parser.rs      # pure: parse a yt-dlp stdout line -> Progress
    │
    └── ui/                         # PRESENTATION (egui widgets)
        ├── mod.rs
        ├── download_form.rs        # URL input + format selector + submit
        └── status_view.rs          # progress bar + speed/ETA + stage/errors
```

### Layer responsibilities

| Layer | Contains | May depend on | Must NOT |
|---|---|---|---|
| `domain` | Value objects, enums, job/progress types | nothing (std only) | know about egui, processes, fs |
| `application` | Use cases, port traits | `domain` | import egui or `std::process` directly |
| `infrastructure` | yt-dlp/ffmpeg adapters, parsers, arg builder | `domain`, `application` (to impl ports) | contain UI code |
| `ui` | egui components, view state | `application`, `domain` | spawn processes directly |
| `main.rs` | Composition root (wiring) | everything | contain business logic |

**Ports (traits) live in `application`, adapters in `infrastructure`.** Example:
`application::ports::MediaDownloader` is a trait; `infrastructure::YtDlpDownloader`
implements it. The UI/use case talk to the trait, so the core is testable with a
fake downloader and yt-dlp can be swapped without touching domain or UI.

---

## 6. Concurrency Model

egui runs a UI loop on the main thread. A download must **never** block it or
the window freezes. Pattern:

```
[UI thread / eframe]                     [worker thread]
  submit()  ------ spawn thread ------>  DownloadService::run()
      |                                        |
      |   <---- mpsc::channel<Progress> -------|  (yt-dlp stdout -> Progress)
      |                                        |
  each frame: drain the channel,          on finish: send Done/Error
  update the progress bar + repaint
```

- The worker thread runs the use case (which drives yt-dlp via the adapter).
- Progress flows back through an `mpsc::Sender<Progress>` (a `ProgressSink`
  port). This is the native equivalent of the SSE stream in the web version.
- The UI drains the receiver every frame and calls `ctx.request_repaint()`
  while a job is active.

---

## 7. Domain Knowledge Carried Over (hard-won gotchas)

These cost real debugging time in the reference app. Bake them in from day one:

1. **`--no-playlist` always.** A watch URL with `&list=RD...` (a radio mix) is
   effectively infinite; without this flag yt-dlp tries to download the whole
   list and hangs forever.
2. **No shell, ever.** Build an argument *vector* and pass it to the process
   directly (`Command::new(bin).args(&[...])`). Never interpolate the URL into
   a shell string. Keep the URL as a single, last argument.
3. **Progress parsing.** Invoke yt-dlp with
   `--newline --progress-template "%(progress._percent_str)s|%(progress._speed_str)s|%(progress._eta_str)s"`
   and parse each stdout line (pipe-delimited). `[Merger]` / `[ExtractAudio]`
   lines mark the **processing** stage (no clean percent — show "Processing…").
4. **Non-ASCII titles.** Filenames may contain accents/CJK/emoji. Sanitize for
   filesystem safety; do not assume ASCII. (In the web app this crashed an HTTP
   header — the desktop equivalent is filesystem path handling.)
5. **Temp-file lifecycle.** Write to a unique temp path, move/save to the final
   destination only on success, and clean up on error/cancel.
6. **Filename.** yt-dlp's reported name may carry the source container
   extension; set the final extension explicitly (avoid `name.webm.mp3`).
7. **Explicit binary paths beat ambient PATH** (see section 4).

---

## 8. Crates (initial)

| Crate | Purpose |
|---|---|
| `eframe` / `egui` | GUI framework + widgets |
| `url` | URL validation (domain layer input) |
| `rfd` | Native file/folder picker (choose where to save) |
| `thiserror` | Ergonomic error types in domain/application |
| `dirs` | Locate default download / config directories |

(Async runtime intentionally omitted for MVP — `std::thread` + `mpsc` is enough.)

---

## 9. MVP Scope (build order)

1. `domain`: `Format`, `MediaUrl`, `DownloadJob` / `Progress` / `Stage`.
2. `infrastructure`: `arg_builder` (pure) + `progress_parser` (pure) — **unit
   tested first** (these are the highest-value, purest seams).
3. `application`: `ports` (traits) + `DownloadService`.
4. `infrastructure`: `YtDlpDownloader` + `BinaryProbe` adapters.
5. `ui`: `download_form` + `status_view`; wire the channel in `app.rs`.
6. `main.rs`: composition root + preflight check.
7. Build & smoke test on Arch and Windows.

---

## 10. Build & Distribution

- Dev: `cargo run`
- Release (per OS — cross-platform means per-target builds):
  - Arch:   `cargo build --release`  -> `target/release/<bin>`
  - Windows: build on Windows (or cross-compile to
    `x86_64-pc-windows-gnu`/`-msvc`) -> `<bin>.exe`
- Optional: bundle `yt-dlp`/`ffmpeg` into `assets/` and load relative to the
  executable for a truly portable build.

---

## 11. Testing

- Strong unit coverage on the **pure** seams: `arg_builder`, `progress_parser`,
  `MediaUrl` validation, format mapping. (Rust `#[cfg(test)]` modules.)
- The `MediaDownloader` port lets us test `DownloadService` with a fake adapter
  (no real yt-dlp needed).
- Manual smoke test on both OSes before tagging a release.

---

## 12. Next Steps

1. Open the new window + init the repo from this folder.
2. Run `sdd-init` in the new project (detects Rust/cargo, sets up persistence).
3. `/sdd-explore` -> `/sdd-new` to formalize the MVP into spec/design/tasks.
4. Implement in the build order above (section 9), TDD on the pure seams.

> Reference implementation (do not copy code, reuse ideas):
> `converter` — Next.js video-downloader with SSE progress, fully archived in Engram.
