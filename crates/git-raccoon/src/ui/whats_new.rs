//! The "What's new" window, after an update (or from the palette).

use eframe::egui::{self, RichText};

use crate::changelog::Notes;
use crate::theme;
use crate::ui::widgets;

/// Shows the notes. Returns `false` once it's closed.
pub fn show(ctx: &egui::Context, notes: &[Notes]) -> bool {
    let mut open = true;
    let modal = egui::Modal::new(egui::Id::new("whats-new")).show(ctx, |ui| {
        ui.set_width(520.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("What's new").strong().size(16.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::icon_button(ui, egui_phosphor::regular::X, "Close what's new").clicked()
                {
                    open = false;
                }
            });
        });
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .max_height(420.0)
            .show(ui, |ui| {
                for n in notes {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!("Version {}", n.version))
                            .strong()
                            .color(theme::accent()),
                    );
                    ui.add_space(2.0);
                    for line in &n.lines {
                        let (bullet, text) = match line.strip_prefix("- ") {
                            Some(rest) => (true, rest),
                            None => (false, line.as_str()),
                        };
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            if bullet {
                                ui.label(RichText::new("•  ").color(theme::muted()));
                            }
                            ui.label(styled(ui, text));
                        });
                    }
                }
            });
        ui.add_space(10.0);
        if ui.button("Nice").clicked() {
            open = false;
        }
    });
    if modal.should_close() {
        open = false;
    }
    open
}

/// A line of Markdown with **bold** parts.
fn styled(ui: &egui::Ui, text: &str) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = ui.available_width();
    let body = theme::body(13.0);
    for (i, part) in text.split("**").enumerate() {
        let bold = i % 2 == 1;
        let format = egui::TextFormat {
            font_id: body.clone(),
            color: if bold { theme::text() } else { theme::muted() },
            ..Default::default()
        };
        job.append(part, 0.0, format);
    }
    job
}
