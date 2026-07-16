//! Status view widget.
//!
//! Renders the current download state: progress bar, speed, ETA, stage label,
//! and Done/Error indicators.

use crate::app::JobState;
use converter_desk::domain::job::Stage;

use eframe::egui;

/// Render the download status area into `ui`.
///
/// This is a read-only view — it takes `&JobState` and never mutates it.
pub fn show(ui: &mut egui::Ui, job_state: &JobState) {
    match job_state {
        JobState::Idle => {
            ui.label(egui::RichText::new("Ready.").weak());
        }

        JobState::Running {
            percent,
            speed,
            eta,
            stage,
        } => {
            // Stage label
            let stage_text = stage_label(stage);
            ui.label(egui::RichText::new(stage_text).strong());

            ui.add_space(4.0);

            // Progress bar — value is 0.0..=1.0
            let fraction = (*percent / 100.0_f32).clamp(0.0, 1.0);
            ui.add(
                egui::ProgressBar::new(fraction)
                    .text(format!("{:.1}%", percent))
                    .animate(true),
            );

            ui.add_space(4.0);

            // Speed and ETA on one line
            if !speed.is_empty() || !eta.is_empty() {
                ui.horizontal(|ui| {
                    if !speed.is_empty() {
                        ui.label(format!("Speed: {speed}"));
                        ui.separator();
                    }
                    if !eta.is_empty() {
                        ui.label(format!("ETA: {eta}"));
                    }
                });
            }
        }

        JobState::Done => {
            ui.label(
                egui::RichText::new("✔  Download complete.")
                    .color(egui::Color32::from_rgb(80, 200, 120))
                    .strong(),
            );
        }

        JobState::Error(msg) => {
            ui.label(
                egui::RichText::new(format!("✖  Error: {msg}"))
                    .color(egui::Color32::from_rgb(220, 60, 60))
                    .strong(),
            );
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn stage_label(stage: &Stage) -> &'static str {
    match stage {
        Stage::Downloading => "Downloading…",
        Stage::Processing => "Processing…",
        Stage::Complete => "Complete",
        Stage::Error => "Error",
    }
}
