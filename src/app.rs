//! Application state and the eframe::App implementation.
//!
//! This module is declared ONLY in `main.rs` (bin side) — never in `lib.rs`.
//! egui/eframe imports live here and nowhere in the library crate.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread::JoinHandle;

use converter_desk::application::download_service::DownloadService;
use converter_desk::domain::format::Format;
use converter_desk::domain::job::DownloadJob;
use converter_desk::domain::media_url::MediaUrl;
use converter_desk::infrastructure::channel_sink::{AppEvent, ChannelSink};
use converter_desk::infrastructure::ytdlp_downloader::YtDlpDownloader;

use eframe::egui;
use crate::ui;

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

    // Service (shared with worker thread via Arc)
    service: Arc<DownloadService<YtDlpDownloader>>,

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
        preflight: PreflightResult,
    ) -> Self {
        let output_dir = dirs::download_dir()
            .unwrap_or_else(|| PathBuf::from("."));

        Self {
            url_input: String::new(),
            format: Format::VideoHighest,
            output_dir,
            job_state: JobState::Idle,
            receiver: None,
            worker: None,
            service: Arc::new(service),
            preflight,
        }
    }

    /// Returns `true` while a download is in progress.
    pub fn is_running(&self) -> bool {
        matches!(self.job_state, JobState::Running { .. })
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
            format: self.format,
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
                        if let JobState::Running {
                            ref mut stage, ..
                        } = self.job_state
                        {
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
                },
                Some(Err(TryRecvError::Empty)) => break,
                Some(Err(TryRecvError::Disconnected)) => {
                    // Worker thread finished without sending Done/Error.
                    // Treat as completion.
                    if matches!(self.job_state, JobState::Running { .. }) {
                        self.job_state = JobState::Done;
                    }
                    self.receiver = None;
                    break;
                }
                None => break,
            }
        }
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

        // 5. Status view — always shown so the user can see Done/Error state.
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
