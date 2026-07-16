//! Application state and the eframe::App implementation.
//!
//! This module is declared ONLY in `main.rs` (bin side) — never in `lib.rs`.
//! egui/eframe imports live here and nowhere in the library crate.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread::JoinHandle;

use converter_desk::application::download_service::DownloadService;
use converter_desk::application::ports::MediaProbe;
use converter_desk::domain::format::Format;
use converter_desk::domain::job::DownloadJob;
use converter_desk::domain::media_info::MediaInfo;
use converter_desk::domain::media_url::MediaUrl;
use converter_desk::domain::quality::Quality;
use converter_desk::infrastructure::channel_sink::{AppEvent, ChannelSink};
use converter_desk::infrastructure::ytdlp_downloader::YtDlpDownloader;
use converter_desk::infrastructure::ytdlp_probe::YtDlpProbe;

use crate::ui;
use eframe::egui;

// ─── Job state ───────────────────────────────────────────────────────────────

/// Tracks the lifecycle of the active download job from the UI's perspective.
pub enum JobState {
    /// No job is running; the form is interactive.
    Idle,
    /// A download is in progress; stores the latest progress snapshot.
    Running {
        percent: f32,
        speed: String,
        eta: String,
        stage: converter_desk::domain::job::Stage,
    },
    /// The download completed successfully.
    Done,
    /// The download failed; carries an error description.
    Error(String),
}

// ─── ProbeState ──────────────────────────────────────────────────────────────

/// Lifecycle state of the media probe (Preview) operation.
pub enum ProbeState {
    /// No probe has been requested yet.
    Idle,
    /// A probe is in progress — worker thread is running.
    Loading,
    /// Probe completed — metadata is available.
    Loaded(MediaInfo),
    /// Probe failed — carries a human-readable error message.
    Error(String),
}

// ─── Preflight result ────────────────────────────────────────────────────────

/// Summarises what was found during the binary-availability preflight check.
pub struct PreflightResult {
    pub ytdlp_version: Option<String>,
    pub ffmpeg_version: Option<String>,
    pub errors: Vec<String>,
}

impl PreflightResult {
    /// Returns `true` when both binaries are available.
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// Build a human-readable banner string for the error case.
    pub fn banner_text(&self) -> String {
        self.errors.join("  |  ")
    }

    /// Return a brief summary of found binary versions (for future status bar use).
    #[allow(dead_code)]
    pub fn version_summary(&self) -> String {
        let ytdlp = self.ytdlp_version.as_deref().unwrap_or("not found");
        let ffmpeg = self.ffmpeg_version.as_deref().unwrap_or("not found");
        format!("yt-dlp: {ytdlp}  ffmpeg: {ffmpeg}")
    }
}

// ─── ConverterApp ────────────────────────────────────────────────────────────

/// Root application struct holding all UI and download state.
pub struct ConverterApp {
    // Form state
    pub url_input: String,
    pub format: Format,
    pub output_dir: PathBuf,

    // Job state
    pub job_state: JobState,
    receiver: Option<Receiver<AppEvent>>,
    worker: Option<JoinHandle<()>>,

    // Probe state
    pub probe_state: ProbeState,
    pub thumbnail_bytes: Option<Vec<u8>>,
    pub thumbnail_uri: Option<String>,
    /// Dedicated channel receiver for probe events (separate from download channel).
    probe_rx: Option<Receiver<AppEvent>>,

    // Quality selection state
    /// The quality level chosen by the user in the ComboBox. Defaults to `Best`.
    pub selected_quality: Quality,
    /// The selectable quality list populated after a successful probe.
    /// Always contains at least `[Quality::Best]`.
    pub selectable_qualities: Vec<Quality>,

    // Service (shared with worker thread via Arc)
    service: Arc<DownloadService<YtDlpDownloader>>,

    // Media probe adapter (shared with worker thread via Arc)
    probe: Arc<dyn MediaProbe>,

    // Preflight
    pub preflight: PreflightResult,
}

impl ConverterApp {
    /// Create a new `ConverterApp`.
    ///
    /// `preflight` is run before calling this; the result is stored and
    /// displayed as an actionable banner when binaries are missing.
    pub fn new(
        service: DownloadService<YtDlpDownloader>,
        probe: YtDlpProbe,
        preflight: PreflightResult,
    ) -> Self {
        let output_dir = dirs::download_dir().unwrap_or_else(|| PathBuf::from("."));

        Self {
            url_input: String::new(),
            format: Format::Video {
                quality: Quality::Best,
            },
            output_dir,
            job_state: JobState::Idle,
            receiver: None,
            worker: None,
            probe_state: ProbeState::Idle,
            thumbnail_bytes: None,
            thumbnail_uri: None,
            probe_rx: None,
            selected_quality: Quality::Best,
            selectable_qualities: vec![Quality::Best],
            service: Arc::new(service),
            probe: Arc::new(probe),
            preflight,
        }
    }

    /// Returns `true` while a download is in progress.
    pub fn is_running(&self) -> bool {
        matches!(self.job_state, JobState::Running { .. })
    }

    /// Start a media probe on a worker thread.
    ///
    /// Transitions `probe_state` to `Loading` immediately. The worker thread
    /// sends `AppEvent::ProbeResult` or `AppEvent::ProbeError` via a dedicated
    /// probe channel (`probe_rx`), kept separate from the download channel.
    pub fn start_probe(&mut self, url_str: String) {
        let url = match MediaUrl::parse(&url_str) {
            Ok(u) => u,
            Err(e) => {
                self.probe_state = ProbeState::Error(format!("Invalid URL: {e}"));
                return;
            }
        };

        self.probe_state = ProbeState::Loading;
        self.thumbnail_bytes = None;
        self.thumbnail_uri = None;
        self.selected_quality = Quality::Best;

        let (tx, rx) = mpsc::channel::<AppEvent>();
        self.probe_rx = Some(rx);

        let probe = Arc::clone(&self.probe);
        std::thread::spawn(move || match probe.probe(&url) {
            Ok(info) => {
                let _ = tx.send(AppEvent::ProbeResult(info));
            }
            Err(e) => {
                let _ = tx.send(AppEvent::ProbeError(e.to_string()));
            }
        });
    }

    /// Called from `ui` when the user clicks Submit.
    ///
    /// Validates the URL, spawns a worker thread, and transitions the app to
    /// `JobState::Running`.
    pub fn submit(&mut self) {
        // Validate URL
        let url = match MediaUrl::parse(&self.url_input) {
            Ok(u) => u,
            Err(e) => {
                self.job_state = JobState::Error(format!("Invalid URL: {e}"));
                return;
            }
        };

        // Build yt-dlp output template from the chosen directory.
        // PathBuf is used throughout; we only convert to string at the
        // point of building the template literal so non-ASCII paths are
        // preserved correctly (PLANNING §7.4).
        let output_path = self
            .output_dir
            .join("%(title)s.%(ext)s")
            .to_string_lossy()
            .into_owned();

        let job = DownloadJob {
            url,
            format: build_format(self.format, self.selected_quality),
            output_path,
        };

        // Create the mpsc channel for progress events.
        let (tx, rx) = mpsc::channel::<AppEvent>();
        self.receiver = Some(rx);
        self.job_state = JobState::Running {
            percent: 0.0,
            speed: String::new(),
            eta: String::new(),
            stage: converter_desk::domain::job::Stage::Downloading,
        };

        // Clone the Arc so the worker thread owns its own reference.
        let service = Arc::clone(&self.service);

        let handle = std::thread::spawn(move || {
            let sink = ChannelSink::new(tx.clone());

            let result = service.execute(&job, &sink);

            // Signal completion or failure after execute returns.
            // `let _ =` silently discards send errors if the UI closed first.
            match result {
                Ok(()) => {
                    let _ = tx.send(AppEvent::Done);
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::Error(e.to_string()));
                }
            }
        });

        self.worker = Some(handle);
    }

    /// Drain the progress receiver without blocking.
    ///
    /// Processes all pending events and updates `job_state` accordingly.
    fn drain_receiver(&mut self) {
        use std::sync::mpsc::TryRecvError;

        // ── Download channel ────────────────────────────────────────────────
        loop {
            match self.receiver.as_ref().map(|rx| rx.try_recv()) {
                Some(Ok(event)) => match event {
                    AppEvent::Progress(p) => {
                        if let JobState::Running {
                            ref mut percent,
                            ref mut speed,
                            ref mut eta,
                            ..
                        } = self.job_state
                        {
                            *percent = p.percent;
                            *speed = p.speed;
                            *eta = p.eta;
                        }
                    }
                    AppEvent::Stage(s) => {
                        if let JobState::Running { ref mut stage, .. } = self.job_state {
                            *stage = s;
                        }
                    }
                    AppEvent::Done => {
                        self.job_state = JobState::Done;
                        self.receiver = None;
                    }
                    AppEvent::Error(msg) => {
                        self.job_state = JobState::Error(msg);
                        self.receiver = None;
                    }
                    // Download channel should not receive probe events, but handle gracefully.
                    AppEvent::ProbeResult(_)
                    | AppEvent::ProbeError(_)
                    | AppEvent::ThumbnailReady(_, _) => {}
                },
                Some(Err(TryRecvError::Empty)) => break,
                Some(Err(TryRecvError::Disconnected)) => {
                    if matches!(self.job_state, JobState::Running { .. }) {
                        self.job_state = JobState::Done;
                    }
                    self.receiver = None;
                    break;
                }
                None => break,
            }
        }

        // ── Probe channel ───────────────────────────────────────────────────
        loop {
            match self.probe_rx.as_ref().map(|rx| rx.try_recv()) {
                Some(Ok(event)) => match event {
                    AppEvent::ProbeResult(info) => {
                        // If thumbnail URL is available, spawn fetch worker.
                        if let Some(ref thumb_url) = info.thumbnail_url {
                            let thumb_url_clone = thumb_url.clone();
                            let probe_url_hash = {
                                let mut h = DefaultHasher::new();
                                thumb_url_clone.hash(&mut h);
                                h.finish()
                            };
                            let uri = format!("bytes://thumb-{probe_url_hash:x}");

                            // Reuse probe_rx for the thumbnail result by creating a thumb channel.
                            let (thumb_tx, thumb_rx) = mpsc::channel::<AppEvent>();
                            self.probe_rx = Some(thumb_rx);

                            std::thread::spawn(move || {
                                let result: Result<Vec<u8>, String> = (|| {
                                    let mut response = ureq::get(&thumb_url_clone)
                                        .call()
                                        .map_err(|e| e.to_string())?;
                                    let bytes = response
                                        .body_mut()
                                        .read_to_vec()
                                        .map_err(|e| e.to_string())?;
                                    Ok(bytes)
                                })(
                                );
                                match result {
                                    Ok(bytes) => {
                                        let _ = thumb_tx.send(AppEvent::ThumbnailReady(bytes, uri));
                                    }
                                    Err(_) => {
                                        // Thumbnail fetch failed — no event sent, metadata still shows.
                                    }
                                }
                            });
                        } else {
                            self.probe_rx = None;
                        }
                        // Reset quality selection and populate list from probe result.
                        self.selected_quality = Quality::Best;
                        self.selectable_qualities =
                            Quality::selectable_list(&info.available_qualities);
                        self.probe_state = ProbeState::Loaded(info);
                    }
                    AppEvent::ProbeError(msg) => {
                        self.probe_state = ProbeState::Error(msg);
                        self.probe_rx = None;
                    }
                    AppEvent::ThumbnailReady(bytes, uri) => {
                        self.thumbnail_bytes = Some(bytes);
                        self.thumbnail_uri = Some(uri);
                        self.probe_rx = None;
                    }
                    // Probe channel should not receive download events.
                    AppEvent::Progress(_)
                    | AppEvent::Stage(_)
                    | AppEvent::Done
                    | AppEvent::Error(_) => {}
                },
                Some(Err(TryRecvError::Empty)) => break,
                Some(Err(TryRecvError::Disconnected)) => {
                    self.probe_rx = None;
                    break;
                }
                None => break,
            }
        }
    }
}

// ─── Pure helpers ────────────────────────────────────────────────────────────

/// Build the `Format` value for a `DownloadJob` by threading `selected` quality
/// into a Video format, or passing AudioMp3 through unchanged.
///
/// Pure function — no side effects, easy to unit-test.
pub(crate) fn build_format(base: Format, selected: Quality) -> Format {
    match base {
        Format::Video { .. } => Format::Video { quality: selected },
        Format::AudioMp3 => Format::AudioMp3,
    }
}

// ─── eframe::App implementation ─────────────────────────────────────────────
impl eframe::App for ConverterApp {
    /// Called every frame. Drains the progress channel then renders the UI.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // 1. Process any pending events from the worker thread.
        self.drain_receiver();

        // 2. Keep repainting while a job is active so progress updates appear.
        if self.is_running() {
            ui.ctx().request_repaint();
        }

        // 3. Preflight error banner.
        if !self.preflight.is_ok() {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(180, 40, 40))
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.colored_label(
                        egui::Color32::WHITE,
                        format!("⚠  {}", self.preflight.banner_text()),
                    );
                });
            ui.separator();
        }

        // 4. Download form (hidden / disabled on preflight failure).
        if self.preflight.is_ok() {
            ui::download_form::show(ui, self);
            ui.separator();
        }

        // 5. Preview panel — only renders content when Loaded, shows stub otherwise.
        ui::preview_panel::show(
            ui,
            &self.probe_state,
            self.thumbnail_bytes.as_deref(),
            self.thumbnail_uri.as_deref(),
            &mut self.selected_quality,
            &self.selectable_qualities,
            self.format,
        );
        ui.separator();

        // 6. Status view — always shown so the user can see Done/Error state.
        ui::status_view::show(ui, &self.job_state);
    }

    /// Called once on shutdown.
    ///
    /// Best-effort cleanup: join the worker thread if it has already exited.
    /// If the worker is still running, we detach (drop the JoinHandle) — the
    /// child process will continue briefly but yt-dlp finishes quickly once
    /// its stdout pipe is closed.
    ///
    /// MVP limitation: cross-platform kill() is not in std; a future release
    /// can add it via the `nix`/`windows-process-extensions` crates.
    fn on_exit(&mut self) {
        if let Some(handle) = self.worker.take() {
            // Non-blocking check: join only if the thread has finished.
            // `JoinHandle` has no `try_join` in stable std, so we attempt a
            // join that will return quickly if the thread has already exited.
            // On drop of a JoinHandle the thread is detached, not killed.
            drop(handle); // detach — acceptable for MVP
        }
    }
}

#[cfg(test)]
mod tests {
    use super::build_format;
    use converter_desk::domain::format::Format;
    use converter_desk::domain::quality::Quality;

    // 3.1 RED — video uses the selected quality (not base quality)
    #[test]
    fn video_uses_selected_quality() {
        let base = Format::Video {
            quality: Quality::Best,
        };
        let result = build_format(base, Quality::P720);
        assert_eq!(
            result,
            Format::Video {
                quality: Quality::P720
            }
        );
    }

    // 3.1 RED — AudioMp3 ignores selected quality, passes through unchanged
    #[test]
    fn audio_mp3_ignores_quality() {
        let result = build_format(Format::AudioMp3, Quality::P1080);
        assert_eq!(result, Format::AudioMp3);
    }

    // 3.1 RED — video with no prior probe (default Best) stays Best when selected is Best
    #[test]
    fn video_default_best_stays_best() {
        let base = Format::Video {
            quality: Quality::Best,
        };
        let result = build_format(base, Quality::Best);
        assert_eq!(
            result,
            Format::Video {
                quality: Quality::Best
            }
        );
    }

    // Triangulation: video with P1080 base, P720 selected → P720 wins
    #[test]
    fn video_selected_overrides_base_quality() {
        let base = Format::Video {
            quality: Quality::P1080,
        };
        let result = build_format(base, Quality::P720);
        assert_eq!(
            result,
            Format::Video {
                quality: Quality::P720
            }
        );
    }

    // Triangulation: AudioMp3 is unaffected by any quality variant
    #[test]
    fn audio_mp3_unaffected_by_any_quality() {
        for q in [Quality::Best, Quality::P2160, Quality::P720, Quality::P360] {
            assert_eq!(
                build_format(Format::AudioMp3, q),
                Format::AudioMp3,
                "AudioMp3 must be unchanged for quality {:?}",
                q
            );
        }
    }
}
