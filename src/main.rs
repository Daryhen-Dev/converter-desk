//! Composition root for converter-desk.
//!
//! Responsibilities:
//!   1. Resolve yt-dlp and ffmpeg binary paths via `resolve_binary_path`.
//!   2. Run a preflight availability check for both binaries.
//!   3. Construct the `YtDlpDownloader` adapter and `DownloadService`.
//!   4. Build a `ConverterApp` with the preflight result.
//!   5. Hand off to `eframe::run_native`.
//!
//! On preflight failure the window still opens with an actionable error banner.
//! The user is shown which binary is missing and how to install it.
//!
//! `mod app` and `mod ui` are declared HERE ONLY — never in `lib.rs`.

mod app;
mod ui;

use app::{ConverterApp, PreflightResult};
use converter_desk::application::download_service::DownloadService;
use converter_desk::application::ports::BinaryProbe;
use converter_desk::infrastructure::binary_probe::{BinaryProbeImpl, resolve_binary_path};
use converter_desk::infrastructure::ytdlp_downloader::YtDlpDownloader;
use converter_desk::infrastructure::ytdlp_probe::YtDlpProbe;

use eframe::egui;

fn main() -> eframe::Result {
    // ── Step 1: Resolve binary paths ────────────────────────────────────────
    let ytdlp_path = resolve_binary_path("yt-dlp", "YT_DLP_PATH");

    // ── Step 2: Preflight binary probe ───────────────────────────────────────
    let probe = BinaryProbeImpl;

    let (ytdlp_version, ytdlp_errors) = match ytdlp_path.as_ref() {
        Some(_) => match probe.check_available("yt-dlp") {
            Ok(v) => (Some(v), vec![]),
            Err(e) => (None, vec![format!(
                "yt-dlp is not working: {e}. Install: winget install yt-dlp  /  pacman -S yt-dlp"
            )]),
        },
        None => (None, vec![
            "yt-dlp not found. Install: winget install yt-dlp  /  pacman -S yt-dlp".to_string()
        ]),
    };

    let (ffmpeg_version, ffmpeg_errors) = match probe.check_available("ffmpeg") {
        Ok(v) => (Some(v), vec![]),
        Err(_) => (None, vec![
            "ffmpeg not found. Install: winget install ffmpeg  /  pacman -S ffmpeg".to_string()
        ]),
    };

    let mut all_errors = ytdlp_errors;
    all_errors.extend(ffmpeg_errors);

    let preflight = PreflightResult {
        ytdlp_version,
        ffmpeg_version,
        errors: all_errors,
    };

    // ── Step 3: Build the downloader adapter and service ────────────────────
    // Reuse the path resolved in step 1; if None, fall back to "yt-dlp" (bare
    // name) so the binary path stored in the adapter is still valid.
    // On preflight failure the UI disables the form so execute() won't be
    // called — but the service must still be constructed for compilation.
    let effective_ytdlp_path =
        ytdlp_path.unwrap_or_else(|| std::path::PathBuf::from("yt-dlp"));

    let downloader = YtDlpDownloader::new(effective_ytdlp_path.clone());
    let service = DownloadService::new(downloader);
    let probe = YtDlpProbe::new(effective_ytdlp_path);

    // ── Step 4: Construct the app ────────────────────────────────────────────
    let app = ConverterApp::new(service, probe, preflight);

    // ── Step 5: Run the eframe event loop ────────────────────────────────────
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Converter Desk")
            .with_inner_size([520.0, 340.0])
            .with_min_inner_size([400.0, 260.0]),
        ..eframe::NativeOptions::default()
    };

    eframe::run_native(
        "Converter Desk",
        native_options,
        Box::new(move |cc| {
            // Install image loaders INSIDE the run_native closure to access CreationContext.
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
}
