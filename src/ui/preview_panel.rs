//! Preview panel widget.
//!
//! Renders media metadata (thumbnail, title, duration, uploader, quality)
//! when ProbeState is Loaded. Shows stub text for other states.

use eframe::egui;

use crate::app::ProbeState;
use converter_desk::domain::format::Format;
use converter_desk::domain::quality::Quality;

/// Format a duration in seconds as `mm:ss`.
///
/// Returns `"--:--"` if `duration_secs` is `None`.
fn format_duration(duration_secs: Option<f64>) -> String {
    match duration_secs {
        None => "--:--".to_string(),
        Some(secs) => {
            let total_secs = secs as u64;
            let mm = total_secs / 60;
            let ss = total_secs % 60;
            format!("{mm:02}:{ss:02}")
        }
    }
}

/// Render the preview panel.
///
/// - When `state` is `ProbeState::Loaded`, shows thumbnail (or placeholder),
///   title, duration, uploader, and an interactive Quality ComboBox.
/// - The Quality ComboBox is hidden when `format` is `Format::AudioMp3`.
/// - For other states shows appropriate stub text.
///
/// `thumbnail_bytes` and `thumbnail_uri` are both `Some` if a thumbnail was
/// fetched successfully; both are `None` otherwise.
pub fn show(
    ui: &mut egui::Ui,
    state: &ProbeState,
    thumbnail_bytes: Option<&[u8]>,
    thumbnail_uri: Option<&str>,
    selected_quality: &mut Quality,
    qualities: &[Quality],
    format: Format,
) {
    match state {
        ProbeState::Idle => {
            ui.label(egui::RichText::new("Enter a URL and click Preview.").weak());
        }
        ProbeState::Loading => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Probing…");
            });
        }
        ProbeState::Error(msg) => {
            ui.label(
                egui::RichText::new(format!("⚠  Probe error: {msg}"))
                    .color(egui::Color32::from_rgb(220, 60, 60)),
            );
        }
        ProbeState::Loaded(info) => {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    // ── Thumbnail ────────────────────────────────────────────
                    if let (Some(bytes), Some(uri)) = (thumbnail_bytes, thumbnail_uri) {
                        // Use egui's Image widget with from_bytes — requires install_image_loaders
                        let img = egui::Image::from_bytes(uri.to_owned(), bytes.to_vec())
                            .max_width(180.0)
                            .max_height(101.0); // 16:9 for 180px wide
                        ui.add(img);
                    } else {
                        // Placeholder
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(180.0, 101.0), egui::Sense::hover());
                        ui.painter()
                            .rect_filled(rect, 4.0, egui::Color32::from_gray(60));
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "No thumbnail",
                            egui::FontId::proportional(12.0),
                            egui::Color32::from_gray(160),
                        );
                    }

                    ui.add_space(8.0);

                    // ── Metadata ─────────────────────────────────────────────
                    ui.vertical(|ui| {
                        // Title
                        ui.label(egui::RichText::new(&info.title).strong().heading());
                        ui.add_space(4.0);

                        // Duration
                        let duration_str = format_duration(info.duration_secs);
                        ui.horizontal(|ui| {
                            ui.label("Duration:");
                            ui.label(egui::RichText::new(duration_str).monospace());
                        });

                        // Uploader
                        if let Some(ref uploader) = info.uploader {
                            ui.horizontal(|ui| {
                                ui.label("Uploader:");
                                ui.label(uploader.as_str());
                            });
                        }

                        ui.add_space(4.0);

                        // Quality ComboBox — interactive; hidden for AudioMp3
                        if matches!(format, Format::Video { .. }) {
                            ui.horizontal(|ui| {
                                ui.label("Quality:");
                                egui::ComboBox::from_id_salt("preview_quality_selector")
                                    .selected_text(selected_quality.to_string())
                                    .show_ui(ui, |ui| {
                                        for q in qualities {
                                            ui.selectable_value(
                                                selected_quality,
                                                *q,
                                                q.to_string(),
                                            );
                                        }
                                    });
                            });
                        }
                    });
                });
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::format_duration;

    #[test]
    fn none_returns_placeholder() {
        assert_eq!(format_duration(None), "--:--");
    }

    #[test]
    fn formats_minutes_and_seconds_zero_padded() {
        assert_eq!(format_duration(Some(5.0)), "00:05");
        assert_eq!(format_duration(Some(65.0)), "01:05");
        assert_eq!(format_duration(Some(193.0)), "03:13");
    }

    #[test]
    fn truncates_fractional_seconds() {
        assert_eq!(format_duration(Some(9.9)), "00:09");
    }
}
