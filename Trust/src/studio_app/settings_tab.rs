//! The Studio's ⚙ Settings tab: Theater's own settings in one place — how
//! targets' outlines are found (mask resolution, scan rate, detection) and
//! the folders Rhai filters are looked for in. Saved as they change.

use std::path::Path;
use std::time::Duration;

use egui::{Color32, RichText};

use super::*;

#[derive(Default)]
pub struct SettingsTab {
    pub status: String,
    /// Detection changed and in use, not saved yet (saved once a slider is let go)
    detection_unsaved: bool,
}

const GREY: Color32 = Color32::from_rgb(148, 163, 184);

/// How a target's outline can be found (TrackingConfig.mode).
const DETECTION_MODES: [(u8, &str, &str); 3] = [
    (0, "Transparency", "What isn't transparent is the target (avatars, game captures with transparency)"),
    (1, "Key colour", "What isn't the key colour is the target (a green screen without a Chroma Key filter)"),
    (2, "Brightness", "What's bright enough is the target"),
];

impl StudioApp {
    pub(super) fn render_settings_tab(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.heading("⚙ Settings");
                ui.label(RichText::new("Saved as you change them.").small().color(GREY));
                ui.add_space(8.0);
                ui.label(RichText::new("⚡ Performance").strong().size(16.0));
                ui.label(
                    RichText::new("What Theater costs OBS. The web pages' options (Tauri sources, the Control Panel's tabs) are in the Control Panel's ⚡ Performance panel.")
                        .small()
                        .color(GREY),
                );
                ui.add_space(4.0);
                self.mask_settings(ui);
                ui.add_space(12.0);
                self.studio_preview_settings(ui);
                ui.add_space(12.0);
                self.studio_screen_settings(ui);
                ui.add_space(16.0);
                self.rhai_folder_settings(ui);
            });
        });
        // A detection slider is saved once it's let go, not every frame of the
        // drag. Copied out first: a read lock held through set_detection's write
        // (a temporary lives to the end of its statement) froze the Studio, and
        // OBS's render thread queued behind it.
        if self.settings.detection_unsaved && !ctx.input(|i| i.pointer.any_down()) {
            let config = *get_state().config.read();
            crate::obs_source::set_detection(config);
            self.settings.detection_unsaved = false;
        }
        // The cost figures are renewed every 10 s
        ctx.request_repaint_after(Duration::from_secs(2));
    }

    fn studio_preview_settings(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(RichText::new("🖼 Studio preview").strong()).default_open(true).show(ui, |ui| {
            ui.label(
                RichText::new("The Studio's picture is a copy of OBS's, made while the Studio is open. Smaller or less often is lighter on OBS.")
                    .small()
                    .color(GREY),
            );
            ui.add_space(4.0);
            let (width, every) = crate::obs_source::studio_preview();
            egui::Grid::new("studio_preview_settings").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.label("Size:");
                let current = crate::obs_source::PREVIEW_SIZES
                    .iter()
                    .find(|(_, w)| *w == width)
                    .map(|(name, _)| name.to_string())
                    .unwrap_or_else(|| format!("{} wide", width));
                egui::ComboBox::from_id_salt("settings_preview_size").selected_text(current).show_ui(ui, |ui| {
                    for (name, w) in crate::obs_source::PREVIEW_SIZES {
                        if ui.selectable_label(width == w, name).clicked() {
                            crate::obs_source::set_studio_preview(w, every);
                            self.settings.status = format!("Studio preview: {}", name);
                        }
                    }
                });
                ui.end_row();

                ui.label("Copied:");
                let current = crate::obs_source::PREVIEW_RATES
                    .iter()
                    .find(|(_, n)| *n == every)
                    .map(|(name, _)| name.to_string())
                    .unwrap_or_else(|| format!("Every {} frames", every));
                egui::ComboBox::from_id_salt("settings_preview_rate").selected_text(current).show_ui(ui, |ui| {
                    for (name, n) in crate::obs_source::PREVIEW_RATES {
                        if ui.selectable_label(every == n, name).clicked() {
                            crate::obs_source::set_studio_preview(width, n);
                            self.settings.status = format!("Studio preview: copied {}", name.to_lowercase());
                        }
                    }
                });
                ui.end_row();
            });
        });
    }

    fn studio_screen_settings(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(RichText::new("🎬 OBS while the Studio is open").strong()).default_open(true).show(ui, |ui| {
            ui.label(
                RichText::new(
                    "The Studio shows Theater's picture; OBS can show a “Studio in use” screen meanwhile. \
                     Theater still draws the scene for the Studio, so masks, redeems and physics carry on. \
                     Minimizing or closing the Studio brings the scene back in OBS.",
                )
                .small()
                .color(GREY),
            );
            ui.add_space(4.0);
            let (mut on, mut even_live) = crate::obs_source::studio_screen_settings();
            let mut changed = ui.checkbox(&mut on, "Show a “Studio in use” screen in OBS").changed();
            ui.add_enabled_ui(on, |ui| {
                ui.indent("studio_screen_live", |ui| {
                    changed |= ui
                        .checkbox(&mut even_live, "…even while streaming, recording or the virtual camera is on")
                        .on_hover_text("Off: while OBS is live, it keeps showing the scene (opening the Studio mid-stream doesn't black it out)")
                        .changed();
                });
            });
            if changed {
                crate::obs_source::set_studio_screen(on, even_live);
            }
        });
    }

    fn mask_settings(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(RichText::new("🎭 Target masks").strong()).default_open(true).show(ui, |ui| {
            ui.label(
                RichText::new(
                    "Theater finds each target's outline in its picture, for what's thrown at it to hit. \
                     Finer and more often cost OBS more every frame; a small target is always scanned at its own size.",
                )
                .small()
                .color(GREY),
            );
            ui.add_space(4.0);
            egui::Grid::new("mask_settings").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.label("Resolution:");
                let pixels = crate::obs_source::mask_pixels();
                let current = crate::obs_source::MASK_RESOLUTIONS
                    .iter()
                    .find(|(_, n)| *n == pixels)
                    .map(|(name, _)| name.to_string())
                    .unwrap_or_else(|| format!("{} pixels", pixels));
                egui::ComboBox::from_id_salt("settings_mask_resolution").selected_text(current).show_ui(ui, |ui| {
                    for (name, n) in crate::obs_source::MASK_RESOLUTIONS {
                        let label = if n == u64::MAX { name.to_string() } else { format!("{} (up to {}k pixels)", name, n / 1000) };
                        if ui.selectable_label(pixels == n, label).clicked() {
                            crate::obs_source::set_mask_pixels(n);
                            self.settings.status = format!("Target masks: {}", name);
                        }
                    }
                });
                ui.end_row();

                ui.label("Scanned:");
                let every = crate::obs_source::scan_every();
                let current = crate::obs_source::MASK_RATES
                    .iter()
                    .find(|(_, n)| *n == every)
                    .map(|(name, _)| name.to_string())
                    .unwrap_or_else(|| format!("Every {} frames", every));
                egui::ComboBox::from_id_salt("settings_mask_rate")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (name, n) in crate::obs_source::MASK_RATES {
                            if ui.selectable_label(every == n, name).clicked() {
                                crate::obs_source::set_scan_every(n);
                                self.settings.status = format!("Target masks: scanned {}", name.to_lowercase());
                            }
                        }
                    })
                    .response
                    .on_hover_text("Less often is lighter on OBS; the mask then follows a moving target a little later");
                ui.end_row();

                // How the outline is found
                let mut config = *get_state().config.read();
                let before = config;
                ui.label("Outline by:");
                let (_, current, _) = DETECTION_MODES.iter().find(|(m, _, _)| *m == config.mode).copied().unwrap_or(DETECTION_MODES[0]);
                egui::ComboBox::from_id_salt("settings_detection_mode").selected_text(current).show_ui(ui, |ui| {
                    for (mode, name, hint) in DETECTION_MODES {
                        ui.selectable_value(&mut config.mode, mode, name).on_hover_text(hint);
                    }
                });
                ui.end_row();
                match config.mode {
                    1 => {
                        ui.label("Key colour:");
                        let mut rgb = [config.chroma_r, config.chroma_g, config.chroma_b];
                        if egui::color_picker::color_edit_button_srgb(ui, &mut rgb).changed() {
                            (config.chroma_r, config.chroma_g, config.chroma_b) = (rgb[0], rgb[1], rgb[2]);
                        }
                        ui.end_row();
                        ui.label("Tolerance:");
                        ui.add(egui::Slider::new(&mut config.threshold, 0..=255))
                            .on_hover_text("How far from the key colour still counts as the key colour");
                        ui.end_row();
                    }
                    2 => {
                        ui.label("Bright enough at:");
                        ui.add(egui::Slider::new(&mut config.threshold, 0..=255));
                        ui.end_row();
                    }
                    _ => {}
                }
                ui.label("Solid from opacity:");
                // From 1: at 0 even fully transparent pixels were the target (a
                // target the size of the canvas became one solid block)
                ui.add(egui::Slider::new(&mut config.alpha_threshold, 1..=255))
                    .on_hover_text("Pixels less opaque than this are never the target");
                ui.end_row();
                if config != before {
                    *get_state().config.write() = config;
                    self.settings.detection_unsaved = true;
                    self.settings.status = "Target masks: outline detection changed".to_string();
                }
            });

            // What it costs, and what each target is scanned at
            ui.add_space(6.0);
            match crate::obs_source::render_summary() {
                Some(s) if s.at.elapsed() < Duration::from_secs(30) => {
                    ui.label(format!(
                        "Target scans cost OBS {:.2} ms a frame ({:.2} scans a frame) of Theater's {:.2} ms, at {:.0} fps (last 10 s).",
                        s.scan_ms, s.scans_per_frame, s.render_ms, s.fps
                    ));
                }
                _ => {
                    ui.label(RichText::new("What the scans cost shows here once Theater has been drawing for 10 s.").color(GREY));
                }
            }
            let targets: Vec<String> = get_state().studio_sources.read().iter().filter(|s| s.is_target).map(|s| s.name.clone()).collect();
            let sizes: Vec<(String, Option<(u32, u32)>)> = match get_state().target_textures.try_read() {
                Some(t) => targets.into_iter().map(|n| {
                    let size = t.get(&n).map(|x| (x.width, x.height));
                    (n, size)
                }).collect(),
                None => Vec::new(),
            };
            for (name, size) in sizes {
                let at = size.map(|(w, h)| format!("scanned at {}×{}", w, h)).unwrap_or_else(|| "not scanned yet".to_string());
                ui.label(RichText::new(format!("🎯 {}: {}", name, at)).small());
            }
        });
    }

    fn rhai_folder_settings(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(RichText::new("📂 Rhai filter folders").strong()).default_open(true).show(ui, |ui| {
            ui.label(
                RichText::new("Theater makes an OBS filter of each .rhai filter in these folders when OBS starts: changes count from the next start.")
                    .small()
                    .color(GREY),
            );
            ui.add_space(4.0);
            let filters_dir = get_filters_dir();
            ui.horizontal(|ui| {
                if ui.small_button("📂").on_hover_text("Open it").clicked() {
                    open_folder(&filters_dir);
                }
                ui.label(RichText::new(filters_dir.display().to_string()).monospace());
                ui.label(RichText::new("(Theater's filters folder)").small().color(GREY));
            });
            for dir in crate::script_filters::remembered_folders() {
                ui.horizontal(|ui| {
                    if ui.small_button("📂").on_hover_text("Open it").clicked() {
                        open_folder(&dir);
                    }
                    ui.label(RichText::new(dir.display().to_string()).monospace());
                    if ui.small_button("✖ Forget").on_hover_text("Stop looking there (its files stay)").clicked() {
                        crate::script_filters::forget_folder(&dir);
                        self.settings.status = format!("{}: no longer looked in from the next OBS start", dir.display());
                    }
                });
            }
            if ui.button("➕ Add a folder…").on_hover_text("Pick a .rhai filter: its folder is added").clicked() {
                let ctx = ui.ctx().clone();
                std::thread::spawn(move || {
                    let Some(path) = pick_file_dialog("Pick a Rhai filter in the folder to add", "Rhai Script | *.rhai") else { return };
                    if let Some(dir) = path.parent() {
                        crate::script_filters::remember_folder(dir);
                    }
                    ctx.request_repaint();
                });
            }
        });
    }
}

fn open_folder(dir: &Path) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer.exe").arg(dir).spawn();
    #[cfg(not(target_os = "windows"))]
    let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
}
