//! Download form widget.
//!
//! Renders the URL input, format selector, output directory picker, and the
//! Submit button. The Submit button is disabled while a job is running
//! (`JobState::Running`) to prevent duplicate submissions.

use eframe::egui;
use egui::ComboBox;

use crate::app::ConverterApp;
use converter_desk::domain::format::Format;
use converter_desk::domain::quality::Quality;

/// Render the download form into `ui`, mutating `app` state in place.
///
/// `app` must be the root application state — the form reads and writes
/// `url_input`, `format`, `output_dir`, and calls `app.submit()`.
pub fn show(ui: &mut egui::Ui, app: &mut ConverterApp) {
    let is_running = app.is_running();

    ui.group(|ui| {
        // ── URL input ────────────────────────────────────────────────────────
        ui.label("URL:");
        ui.add_enabled(
            !is_running,
            egui::TextEdit::singleline(&mut app.url_input)
                .hint_text("https://www.youtube.com/watch?v=…")
                .desired_width(f32::INFINITY),
        );

        ui.add_space(4.0);

        // ── Format selector ──────────────────────────────────────────────────
        ui.label("Format:");
        ui.add_enabled_ui(!is_running, |ui| {
            ComboBox::from_id_salt("format_selector")
                .selected_text(format_label(app.format))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut app.format,
                        Format::Video {
                            quality: Quality::Best,
                        },
                        format_label(Format::Video {
                            quality: Quality::Best,
                        }),
                    );
                    ui.selectable_value(
                        &mut app.format,
                        Format::AudioMp3,
                        format_label(Format::AudioMp3),
                    );
                });
        });

        ui.add_space(4.0);

        // ── Output directory picker ──────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label("Output folder:");
            let dir_label = app.output_dir.to_string_lossy().into_owned();
            ui.label(egui::RichText::new(&dir_label).monospace());

            if ui
                .add_enabled(!is_running, egui::Button::new("Browse…"))
                .clicked()
            {
                // `rfd::FileDialog` is a synchronous native dialog — safe to
                // call from the UI thread. The dialog blocks only while open.
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    app.output_dir = path;
                }
            }
        });

        ui.add_space(8.0);

        // ── Preview + Submit buttons ─────────────────────────────────────────
        ui.horizontal(|ui| {
            // Preview button — disabled while already loading
            let is_loading = matches!(app.probe_state, crate::app::ProbeState::Loading);
            if ui
                .add_enabled(
                    !is_loading,
                    egui::Button::new(if is_loading { "Probing…" } else { "Preview" }),
                )
                .clicked()
            {
                app.start_probe(app.url_input.clone());
            }

            // Download button — enabled for ALL ProbeState variants
            let submit_label = if is_running {
                "Downloading…"
            } else {
                "Download"
            };
            if ui
                .add_enabled(!is_running, egui::Button::new(submit_label))
                .clicked()
            {
                app.submit();
            }
        });
    });
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn format_label(format: Format) -> &'static str {
    match format {
        Format::Video { .. } => "Video (highest quality)",
        Format::AudioMp3 => "Audio (MP3)",
    }
}
