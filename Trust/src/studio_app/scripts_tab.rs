//! The Studio's 📜 Scripts tab: every script Theater or OBS uses, grouped as
//! reactions, redeems and filters (script_catalog), with what's wrong with
//! each, where it's used, its settings, and what can be done with it.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::{Color32, RichText};

use super::*;
use crate::script_catalog::{Catalog, Kind, Lang, ScriptEntry};

/// How often the tab rescans while it's shown.
const RESCAN_EVERY: Duration = Duration::from_secs(3);

#[derive(Default)]
pub struct ScriptsTab {
    catalog: Option<Catalog>,
    scanning: Option<crossbeam_channel::Receiver<Catalog>>,
    last_scan: Option<Instant>,
    /// A script's file, or a redeem's
    selected: Option<PathBuf>,
    /// The script being renamed, and the name typed so far
    renaming: Option<(PathBuf, String)>,
    pub status: String,
}

const RED: Color32 = Color32::from_rgb(239, 68, 68);
const ORANGE: Color32 = Color32::from_rgb(251, 146, 60);
const GREEN: Color32 = Color32::from_rgb(34, 197, 94);
const GREY: Color32 = Color32::from_rgb(148, 163, 184);
const BLUE: Color32 = Color32::from_rgb(96, 165, 250);

fn same_file(a: &Path, b: &Path) -> bool {
    a == b || matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
}

fn lang_badge(lang: Lang) -> RichText {
    match lang {
        Lang::Lua => RichText::new("Lua").small().color(BLUE),
        Lang::Rhai => RichText::new("Rhai").small().color(ORANGE),
    }
}

impl StudioApp {
    /// Scans in the background when the tab needs it; takes a finished scan.
    fn poll_scripts_scan(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.scripts.scanning {
            match rx.try_recv() {
                Ok(catalog) => {
                    // A reaction's file changed: its shader is remade when next drawn
                    if let Some(old) = &self.scripts.catalog {
                        for entry in catalog.entries.iter().filter(|e| e.is(Kind::Reaction)) {
                            let before = old.entries.iter().find(|o| o.path == entry.path).and_then(|o| o.modified);
                            if before.is_some() && before != entry.modified {
                                crate::obs_source::reload_reaction_script(&entry.path);
                                self.scripts.status = format!("{} changed: reloaded", entry.name);
                            }
                        }
                    }
                    self.scripts.catalog = Some(catalog);
                    self.scripts.scanning = None;
                }
                Err(crossbeam_channel::TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(100)),
                Err(crossbeam_channel::TryRecvError::Disconnected) => self.scripts.scanning = None,
            }
        }
        let due = self.scripts.last_scan.is_none_or(|t| t.elapsed() >= RESCAN_EVERY);
        if due && self.scripts.scanning.is_none() {
            self.start_scripts_scan();
        }
        ctx.request_repaint_after(RESCAN_EVERY);
    }

    fn start_scripts_scan(&mut self) {
        let mut dirs = vec![get_filters_dir(), crate::paths::scripts_dir(), crate::paths::user_filters_dir(), crate::paths::user_scripts_dir()];
        dirs.extend(crate::script_filters::script_dirs());
        let extra: Vec<PathBuf> = self
            .lua_effects_list
            .iter()
            .chain(self.custom_redeem_scripts_list.iter())
            .map(|e| e.path.clone())
            .collect();
        let (tx, rx) = crossbeam_channel::bounded(1);
        let spawned = std::thread::Builder::new().name("theater-scripts-scan".into()).spawn(move || {
            let _ = tx.send(crate::script_catalog::scan(&dirs, &extra));
        });
        if spawned.is_ok() {
            self.scripts.scanning = Some(rx);
        }
        self.scripts.last_scan = Some(Instant::now());
    }

    pub(super) fn render_scripts_tab(&mut self, ctx: &egui::Context) {
        self.poll_scripts_scan(ctx);
        let entries: Vec<ScriptEntry> = self.scripts.catalog.as_ref().map(|c| c.entries.clone()).unwrap_or_default();

        egui::SidePanel::left("scripts_list").min_width(300.0).default_width(340.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("📜 Scripts");
                if ui.button("🔄").on_hover_text("Scan again").clicked() {
                    self.start_scripts_scan();
                }
                ui.menu_button("➕ New", |ui| {
                    for (kind, label) in [(Kind::Reaction, "🎯 Reaction (Lua shader)"), (Kind::Redeem, "🎁 Redeem script (Lua)"), (Kind::Filter, "🧩 OBS filter (Rhai)")] {
                        if ui.button(label).clicked() {
                            self.scripts.status = match new_script(kind) {
                                Ok(path) => {
                                    open_in_editor(&path);
                                    self.scripts.selected = Some(path.clone());
                                    self.start_scripts_scan();
                                    let note = if kind == Kind::Filter { " — restart OBS to use it" } else { "" };
                                    format!("Created {}{}", path.display(), note)
                                }
                                Err(e) => format!("Couldn't create it: {}", e),
                            };
                            ui.close_menu();
                        }
                    }
                });
                ui.menu_button("📂 Add file", |ui| {
                    for (target, label, title) in [
                        (FileDialogTarget::LuaEffectScript, "A reaction script…", "Add a Reaction Script"),
                        (FileDialogTarget::CustomRedeemScript, "A redeem script…", "Add a Redeem Script"),
                    ] {
                        if ui.button(label).clicked() {
                            let tx = self.file_dialog_tx.clone();
                            let ctx = ctx.clone();
                            std::thread::spawn(move || {
                                if let Some(path) = pick_file_dialog(title, "Lua Script | *.lua") {
                                    let _ = tx.send(FileDialogResult { target, path });
                                    ctx.request_repaint();
                                }
                            });
                            ui.close_menu();
                        }
                    }
                    if ui.button("An OBS filter (Rhai)…").on_hover_text("Theater looks for Rhai filters in its folder from the next OBS start").clicked() {
                        let ctx = ctx.clone();
                        std::thread::spawn(move || {
                            let Some(path) = pick_file_dialog("Add a Rhai Filter", "Rhai Script | *.rhai") else { return };
                            if let Some(dir) = path.parent() {
                                crate::script_filters::remember_folder(dir);
                            }
                            ctx.request_repaint();
                        });
                        self.scripts.status = "Pick a .rhai filter: its folder is added, and it runs once OBS restarts".to_string();
                        ui.close_menu();
                    }
                });
            });
            if self.scripts.catalog.is_none() {
                ui.label(RichText::new("Looking for scripts…").italics().color(GREY));
            }
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                egui::CollapsingHeader::new(format!("📄 Redeems ({})", self.redeems_list.len()))
                    .id_salt("📄 Redeems")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.label(RichText::new("The redeems themselves (their .json files): what fires, and what it runs").small().color(GREY));
                        for idx in 0..self.redeems_list.len() {
                            self.redeem_row(ui, idx);
                        }
                    });
                for (kind, title, hint) in [
                    (Kind::Redeem, "🎁 Redeem scripts", "Lua a redeem can run as its Custom Lua Script: on_redeem runs when it fires"),
                    (Kind::Reaction, "🎯 Reactions", "Shaders Theater runs on a target when it's hit (a redeem's Impact Reaction)"),
                    (Kind::Filter, "🧩 Filters", "OBS filters and sources: Lua ones OBS loads (Tools → Scripts), Rhai ones Theater hosts (the chat, the 360 filters)"),
                ] {
                    let group: Vec<&ScriptEntry> = entries.iter().filter(|e| e.is(kind)).collect();
                    egui::CollapsingHeader::new(format!("{} ({})", title, group.len()))
                        .id_salt(title)
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.label(RichText::new(hint).small().color(GREY));
                            for entry in group {
                                self.script_row(ui, entry, kind);
                            }
                        });
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let redeem = self.scripts.selected.as_ref().and_then(|p| self.redeems_list.iter().position(|r| r.path == *p));
            let selected = self
                .scripts
                .selected
                .as_ref()
                .and_then(|p| entries.iter().find(|e| same_file(&e.path, p)))
                .cloned();
            match (redeem, selected) {
                (Some(idx), _) => {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.redeem_details(ui, idx, &entries));
                }
                (None, Some(entry)) => {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.script_details(ui, &entry));
                }
                (None, None) => {
                    ui.add_space(20.0);
                    ui.label(RichText::new("Pick a script on the left to see what's wrong with it, where it's used, its settings and what you can do with it.").color(GREY));
                }
            }
        });
    }

    fn script_row(&mut self, ui: &mut egui::Ui, entry: &ScriptEntry, kind: Kind) {
        let hidden = self.is_hidden_from_pickers(entry, kind);
        let selected = self.scripts.selected.as_ref().is_some_and(|p| same_file(p, &entry.path));
        ui.horizontal(|ui| {
            let (icon, color) = if entry.problem.is_some() {
                ("⚠", RED)
            } else if !entry.threats.is_empty() {
                ("🛡", ORANGE)
            } else {
                ("✓", GREEN)
            };
            ui.label(RichText::new(icon).color(color));
            let mut name = RichText::new(&entry.name);
            if hidden {
                name = name.color(GREY).italics();
            }
            if ui.selectable_label(selected, name).clicked() {
                self.scripts.selected = Some(entry.path.clone());
            }
            ui.label(lang_badge(entry.lang));
            if entry.loaded_by_obs {
                ui.label(RichText::new("OBS").small().color(GREY)).on_hover_text("OBS loads it (Tools → Scripts)");
            }
            if hidden {
                ui.label(RichText::new("hidden").small().color(GREY));
            }
        });
    }

    fn redeem_row(&mut self, ui: &mut egui::Ui, idx: usize) {
        let item = &self.redeems_list[idx];
        let selected = self.scripts.selected.as_ref() == Some(&item.path);
        let path = item.path.clone();
        if ui.selectable_label(selected, redeem_choice_label(item)).clicked() {
            self.scripts.selected = Some(path);
            self.scripts.renaming = None;
        }
    }

    /// A redeem (its .json): what it is, what it runs, and where to edit it.
    fn redeem_details(&mut self, ui: &mut egui::Ui, idx: usize, entries: &[ScriptEntry]) {
        let item = &self.redeems_list[idx];
        let (path, filename, name, kind, triggers, dirty) = (
            item.path.clone(),
            item.filename.clone(),
            item.config.name.clone(),
            item.config.redeem_type.clone(),
            item.triggers_input.clone(),
            item.is_dirty,
        );
        let script = item.config.custom_redeem_script.clone().filter(|s| !s.trim().is_empty());
        let reaction = item.config.get_custom_script_enabled().then(|| item.config.get_custom_script_path()).flatten();

        ui.horizontal(|ui| {
            ui.heading(format!("{} {}", redeem_badge(&kind), filename));
            if dirty {
                ui.label(RichText::new("● unsaved changes in the editor").color(ORANGE));
            }
        });
        ui.label(RichText::new(path.display().to_string()).monospace().small().color(GREY));
        ui.horizontal_wrapped(|ui| {
            if ui.button("✏ Edit in Redeem & FX Editor").clicked() {
                self.selected_redeem_index = Some(idx);
                self.active_tab = StudioActiveTab::RedeemEditor;
            }
            if ui.button("📝 Open").on_hover_text("Open its .json file").clicked() {
                open_in_editor(&path);
            }
            if ui.button("📂 Show in folder").clicked() {
                show_in_folder(&path);
            }
        });
        ui.separator();

        ui.label(RichText::new("Redeem").strong());
        let or_none = |s: &str| if s.trim().is_empty() { "(none)".to_string() } else { s.to_string() };
        ui.label(format!("Name: {}", or_none(&name)));
        ui.label(format!("Type: {}", kind));
        ui.label(format!("Triggers: {}", or_none(&triggers)));
        ui.separator();

        // Its scripts, a click away
        ui.label(RichText::new("Runs").strong());
        let mut any = false;
        for (what, raw) in [("🎁 Its script", script), ("🎯 Its Impact Reaction", reaction)] {
            let Some(raw) = raw else { continue };
            any = true;
            let resolved = PathBuf::from(resolve_script_path(&raw));
            let entry = entries.iter().find(|e| same_file(&e.path, &resolved));
            ui.horizontal(|ui| {
                ui.label(format!("{}:", what));
                match entry {
                    Some(e) => {
                        if ui.link(&e.name).on_hover_text(e.path.display().to_string()).clicked() {
                            self.scripts.selected = Some(e.path.clone());
                        }
                    }
                    None if resolved.is_file() => {
                        ui.label(resolved.display().to_string());
                    }
                    None => {
                        ui.label(RichText::new(format!("⚠ missing: {}", raw)).color(RED));
                    }
                }
            });
        }
        if !any {
            ui.label(RichText::new("No script: it does what its settings in the editor say").color(GREY));
        }
    }

    /// The new name being typed for `entry`, with Rename / Cancel.
    fn rename_row(&mut self, ui: &mut egui::Ui, entry: &ScriptEntry) {
        let ext = entry.path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
        let (mut confirm, mut cancel) = (false, false);
        ui.horizontal(|ui| {
            ui.label("New name:");
            if let Some((_, name)) = self.scripts.renaming.as_mut() {
                let field = ui.text_edit_singleline(name);
                if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    confirm = true;
                }
            }
            ui.label(format!(".{}", ext));
            confirm |= ui.button("✓ Rename").clicked();
            cancel = ui.button("Cancel").clicked();
        });
        if cancel {
            self.scripts.renaming = None;
        } else if confirm {
            let name = self.scripts.renaming.as_ref().map(|(_, n)| n.clone()).unwrap_or_default();
            self.scripts.status = match self.rename_script(entry, &name) {
                Ok(done) => {
                    self.scripts.renaming = None;
                    done
                }
                Err(why) => format!("⚠ Not renamed: {}", why),
            };
        }
    }

    /// Renames `entry`'s file to `new_name` (same folder and extension). The
    /// redeems using it (as their script or Impact Reaction), the reaction
    /// list's hidden ones and a running Rhai filter follow it.
    fn rename_script(&mut self, entry: &ScriptEntry, new_name: &str) -> Result<String, String> {
        if entry.loaded_by_obs {
            return Err("OBS loads it by this name (Tools → Scripts)".to_string());
        }
        let old = entry.path.clone();
        let new = renamed_path(&old, new_name)?;
        let stem_owned = new.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let stem = stem_owned.as_str();

        // What points at it, found before the move: a moved file can't be compared
        let points_at = |raw: Option<String>| raw.is_some_and(|s| !s.trim().is_empty() && same_file(Path::new(&resolve_script_path(&s)), &old));
        let script_users: Vec<usize> = (0..self.redeems_list.len())
            .filter(|&i| points_at(self.redeems_list[i].config.custom_redeem_script.clone()))
            .collect();
        let reaction_users: Vec<usize> = (0..self.redeems_list.len())
            .filter(|&i| points_at(self.redeems_list[i].config.get_custom_script_path()))
            .collect();
        let in_effects: Vec<usize> = (0..self.lua_effects_list.len()).filter(|&i| same_file(&self.lua_effects_list[i].path, &old)).collect();
        let in_scripts: Vec<usize> = (0..self.custom_redeem_scripts_list.len())
            .filter(|&i| same_file(&self.custom_redeem_scripts_list[i].path, &old))
            .collect();
        let rhai = crate::script_filters::types().into_iter().find(|t| same_file(&t.path(), &old));

        std::fs::rename(&old, &new).map_err(|e| e.to_string())?;

        let new_str = new.to_string_lossy().into_owned();
        let mut failed = Vec::new();
        for &i in &script_users {
            let item = &mut self.redeems_list[i];
            item.config.custom_redeem_script = Some(new_str.clone());
            // A new redeem that was never saved has no file yet
            if item.path.is_file() {
                if let Err(e) = rewrite_redeem_file(&item.path, |c| c.custom_redeem_script = Some(new_str.clone())) {
                    failed.push(e);
                }
            }
        }
        for &i in &reaction_users {
            let item = &mut self.redeems_list[i];
            set_reaction_script(&mut item.config, &new_str, stem);
            if item.path.is_file() {
                if let Err(e) = rewrite_redeem_file(&item.path, |c| set_reaction_script(c, &new_str, stem)) {
                    failed.push(e);
                }
            }
        }
        let updated: std::collections::BTreeSet<usize> = script_users.iter().chain(&reaction_users).copied().collect();
        if !updated.is_empty() {
            reload_global_redeems();
        }
        // Hidden from the reaction list under its old name: stays hidden
        if self.removed_lua_effects.remove(&effect_file_name(&old)) {
            self.removed_lua_effects.insert(effect_file_name(&new));
            save_removed_lua_effects(&self.removed_lua_effects);
        }
        for i in in_effects {
            self.lua_effects_list[i] = LuaScriptEntry { name: stem.to_string(), path: new.clone() };
        }
        for i in in_scripts {
            self.custom_redeem_scripts_list[i] = LuaScriptEntry { name: stem.to_string(), path: new.clone() };
        }
        if let Some(ty) = rhai {
            crate::script_filters::file_renamed(ty, &new);
        }
        self.scripts.selected = Some(new.clone());
        self.start_scripts_scan();

        let file = |p: &Path| p.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let mut done = format!("Renamed {} → {}", file(&old), file(&new));
        if !updated.is_empty() {
            done.push_str(&format!(" ({} redeem{} updated)", updated.len(), if updated.len() == 1 { "" } else { "s" }));
        }
        if !failed.is_empty() {
            done.push_str(&format!(" — ⚠ couldn't update: {}", failed.join("; ")));
        }
        Ok(done)
    }

    /// Hidden from the redeem editor's pickers (the reaction or script lists).
    fn is_hidden_from_pickers(&self, entry: &ScriptEntry, kind: Kind) -> bool {
        match kind {
            Kind::Reaction => self.removed_lua_effects.contains(&effect_file_name(&entry.path)),
            Kind::Redeem => !self.custom_redeem_scripts_list.iter().any(|e| same_file(&e.path, &entry.path)),
            Kind::Filter => false,
        }
    }

    fn script_details(&mut self, ui: &mut egui::Ui, entry: &ScriptEntry) {
        ui.horizontal(|ui| {
            ui.heading(&entry.name);
            ui.label(lang_badge(entry.lang));
            for kind in &entry.kinds {
                let label = match kind {
                    Kind::Reaction => "🎯 reaction",
                    Kind::Redeem => "🎁 redeem",
                    Kind::Filter => "🧩 filter",
                };
                ui.label(RichText::new(label).small());
            }
        });
        ui.label(RichText::new(entry.path.display().to_string()).monospace().small().color(GREY));

        // What can be done with it
        ui.horizontal_wrapped(|ui| {
            if ui.button("📝 Open").on_hover_text("Open it in its editor").clicked() {
                open_in_editor(&entry.path);
            }
            if ui.button("📂 Show in folder").clicked() {
                show_in_folder(&entry.path);
            }
            if entry.is(Kind::Reaction) && ui.button("🔄 Reload").on_hover_text("Rebuild its shader now (it also reloads by itself when the file changes)").clicked() {
                crate::obs_source::reload_reaction_script(&entry.path);
                self.scripts.status = format!("{}: its shader is rebuilt next time it's drawn", entry.name);
            }
            if entry.lang == Lang::Rhai {
                if let Some(ty) = crate::script_filters::types().into_iter().find(|t| same_file(&t.path(), &entry.path)) {
                    if ui.button("🔄 Reload").on_hover_text("Read it again now (it also reloads by itself when you save it)").clicked() {
                        crate::script_filters::reload_if_changed(ty);
                        self.scripts.status = format!("{}: reloaded", entry.name);
                    }
                }
            }
            for kind in [Kind::Reaction, Kind::Redeem] {
                if !entry.is(kind) {
                    continue;
                }
                let what = if kind == Kind::Reaction { "the Impact Reaction list" } else { "the custom script list" };
                if self.is_hidden_from_pickers(entry, kind) {
                    if ui.button(format!("👁 Show in {}", what)).clicked() {
                        self.show_in_pickers(entry, kind);
                    }
                } else if ui.button(format!("🙈 Hide from {}", what)).on_hover_text("Only from that list: the file stays, and redeems using it keep working").clicked() {
                    self.hide_from_pickers(entry, kind);
                }
            }
            let rename = ui
                .add_enabled(!entry.loaded_by_obs, egui::Button::new("✏ Rename"))
                .on_hover_text("Rename its file: the redeems using it follow")
                .on_disabled_hover_text("OBS loads it by this name (Tools → Scripts): remove it there first, rename it, then add it back");
            if rename.clicked() {
                self.scripts.renaming = Some((entry.path.clone(), entry.name.clone()));
            }
        });
        if self.scripts.renaming.as_ref().is_some_and(|(p, _)| *p == entry.path) {
            self.rename_row(ui, entry);
        }
        ui.separator();

        // What's wrong with it
        ui.label(RichText::new("Status").strong());
        if let Some(problem) = &entry.problem {
            ui.label(RichText::new(format!("⚠ {}", problem)).color(RED));
        }
        if entry.is(Kind::Redeem) {
            if entry.threats.is_empty() {
                ui.label(RichText::new("🛡 Nothing the redeem sandbox blocks").color(GREEN));
            }
            for threat in &entry.threats {
                ui.label(RichText::new(format!("🛡 Blocked when it runs: {} (line {}) — it {}", threat.call, threat.line, threat.why)).color(ORANGE));
            }
            ui.label(RichText::new("Read again each time the redeem fires: edits count right away.").small().color(GREY));
        }
        if entry.is(Kind::Filter) {
            self.filter_status(ui, entry);
        }
        ui.separator();

        // Where it's used
        ui.label(RichText::new("Used by").strong());
        let users = self.script_users(entry);
        if users.is_empty() {
            ui.label(RichText::new("Nothing (in the redeems, or the scene collection as last saved)").color(GREY));
        }
        for (user, redeem) in users {
            match redeem {
                Some(path) => {
                    if ui.link(format!("• {}", user)).on_hover_text("Show this redeem").clicked() {
                        self.scripts.selected = Some(path);
                    }
                }
                None => {
                    ui.label(format!("• {}", user));
                }
            }
        }
        ui.separator();

        // Its settings
        ui.label(RichText::new("Settings").strong());
        let mut any = false;
        for slider in &entry.sliders {
            any = true;
            ui.label(format!("• {} ({}): {} [{} – {}]", slider.label, slider.name, slider.default, slider.min, slider.max));
        }
        for (name, value) in &entry.reaction_settings {
            any = true;
            ui.label(format!("• {} = {}", name, value));
        }
        if entry.lang == Lang::Rhai {
            if let Some(ty) = crate::script_filters::types().into_iter().find(|t| same_file(&t.path(), &entry.path)) {
                let compiled = ty.compiled();
                for (name, value) in &compiled.defaults {
                    any = true;
                    ui.label(format!("• {} = {}", name, value));
                }
            }
        }
        if !any {
            ui.label(RichText::new("None").color(GREY));
        }
    }

    fn filter_status(&self, ui: &mut egui::Ui, entry: &ScriptEntry) {
        let rhai_owner = entry.filter_id.as_ref().and_then(|id| {
            crate::script_filters::types().into_iter().find(|t| t.id() == *id)
        });
        match entry.lang {
            Lang::Rhai => {
                let file = crate::script_filters::files().into_iter().find(|f| same_file(&f.path, &entry.path));
                match (rhai_owner.filter(|t| same_file(&t.path(), &entry.path)), file) {
                    (Some(ty), _) => {
                        let compiled = ty.compiled();
                        if compiled.kind == crate::script_filters::ScriptKind::Dock {
                            ui.label(RichText::new(format!("✓ Running as the dock “{}” (Theater's menu)", compiled.name)).color(GREEN));
                        } else {
                            ui.label(RichText::new(format!("✓ Running in OBS as “{}” — {} in use", compiled.name, ty.instances())).color(GREEN));
                        }
                        if let Some(e) = ty.error.lock().clone() {
                            ui.label(RichText::new(format!("⚠ Its latest edit isn't running (the one before is): {}", e)).color(ORANGE));
                        } else {
                            ui.label(RichText::new("Reloads by itself when you save it.").small().color(GREY));
                        }
                    }
                    (None, Some(file)) => {
                        ui.label(RichText::new(format!("⚠ Not running: {}", file.problem.unwrap_or_default())).color(RED));
                        ui.label(RichText::new("Fix it, then restart OBS: filters are registered when OBS starts.").small().color(GREY));
                    }
                    (None, None) => {
                        ui.label(RichText::new("New since OBS started: restart OBS to use it (filters are registered when OBS starts).").color(ORANGE));
                    }
                }
            }
            Lang::Lua => {
                match (entry.loaded_by_obs, rhai_owner) {
                    (true, Some(ty)) => {
                        ui.label(RichText::new(format!(
                            "⚠ OBS loads it, but its filter id belongs to the Rhai version ({}) now: this copy isn't used. A Lua filter that registers a taken id anyway makes OBS crash on exit (an OBS bug): Theater's own copies step aside, others should be removed from OBS's Tools → Scripts.",
                            ty.path().file_name().unwrap_or_default().to_string_lossy()
                        )).color(ORANGE));
                    }
                    (true, None) => {
                        ui.label("OBS loads it (Tools → Scripts). After editing it, reload it there.");
                    }
                    (false, _) => {
                        ui.label(RichText::new("OBS doesn't load it (not in Tools → Scripts).").color(GREY));
                    }
                }
            }
        }
        if let Some(id) = &entry.filter_id {
            ui.label(RichText::new(format!("Filter id: {}", id)).small().color(GREY));
        }
    }

    /// Where `entry` is used: redeems (as their script or reaction) and, for a
    /// filter, the sources carrying it in the scene collection.
    fn script_users(&self, entry: &ScriptEntry) -> Vec<(String, Option<PathBuf>)> {
        let mut users = Vec::new();
        for item in &self.redeems_list {
            let label = if item.config.name.trim().is_empty() { item.filename.clone() } else { format!("{} ({})", item.config.name, item.filename) };
            if entry.is(Kind::Redeem) {
                if let Some(script) = &item.config.custom_redeem_script {
                    if same_file(Path::new(&resolve_script_path(script)), &entry.path) {
                        users.push((format!("🎁 {} — its script", label), Some(item.path.clone())));
                    }
                }
            }
            if entry.is(Kind::Reaction) && item.config.get_custom_script_enabled() {
                if let Some(script) = item.config.get_custom_script_path() {
                    if same_file(Path::new(&resolve_script_path(&script)), &entry.path) {
                        users.push((format!("🎁 {} — its Impact Reaction", label), Some(item.path.clone())));
                    }
                }
            }
        }
        if let (Some(id), Some(catalog)) = (&entry.filter_id, &self.scripts.catalog) {
            for source in catalog.filter_users.get(id).into_iter().flatten() {
                users.push((format!("🧩 {}", source), None));
            }
        }
        users
    }

    fn hide_from_pickers(&mut self, entry: &ScriptEntry, kind: Kind) {
        match kind {
            Kind::Reaction => {
                self.removed_lua_effects.insert(effect_file_name(&entry.path));
                save_removed_lua_effects(&self.removed_lua_effects);
                self.lua_effects_list.retain(|e| !same_file(&e.path, &entry.path));
                self.selected_lua_effect_index = None;
            }
            Kind::Redeem => {
                self.custom_redeem_scripts_list.retain(|e| !same_file(&e.path, &entry.path));
                self.selected_custom_redeem_index = None;
            }
            Kind::Filter => {}
        }
        self.scripts.status = format!("{} hidden from the list (its file stays)", entry.name);
    }

    fn show_in_pickers(&mut self, entry: &ScriptEntry, kind: Kind) {
        let script = LuaScriptEntry { name: entry.name.clone(), path: entry.path.clone() };
        match kind {
            Kind::Reaction => {
                if self.removed_lua_effects.remove(&effect_file_name(&entry.path)) {
                    save_removed_lua_effects(&self.removed_lua_effects);
                }
                if !self.lua_effects_list.iter().any(|e| same_file(&e.path, &entry.path)) {
                    self.lua_effects_list.push(script);
                }
            }
            Kind::Redeem => {
                if !self.custom_redeem_scripts_list.iter().any(|e| same_file(&e.path, &entry.path)) {
                    self.custom_redeem_scripts_list.push(script);
                }
            }
            Kind::Filter => {}
        }
        self.scripts.status = format!("{} is back in the list", entry.name);
    }
}

/// Where `old` goes when renamed to `new_name` (typed with or without its
/// extension): same folder, same extension. Err: why it can't.
fn renamed_path(old: &Path, new_name: &str) -> Result<PathBuf, String> {
    let ext = old.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let mut stem = new_name.trim();
    let dot_ext = format!(".{}", ext);
    if stem.len() > dot_ext.len() && stem.is_char_boundary(stem.len() - dot_ext.len()) && stem[stem.len() - dot_ext.len()..].eq_ignore_ascii_case(&dot_ext) {
        stem = stem[..stem.len() - dot_ext.len()].trim_end();
    }
    if stem.is_empty() {
        return Err("the name is empty".to_string());
    }
    if stem.chars().any(|c| r#"\/:*?"<>|"#.contains(c) || c.is_control()) || stem.ends_with('.') {
        return Err("a file name can't hold \\ / : * ? \" < > | or end with a dot".to_string());
    }
    let new = old.with_file_name(format!("{}{}", stem, dot_ext));
    if new == old {
        return Err("that's its name already".to_string());
    }
    // Another file of that name (not this one with other capitals)
    if new.exists() && !same_file(&new, old) {
        return Err(format!("{} is there already", new.file_name().unwrap_or_default().to_string_lossy()));
    }
    Ok(new)
}

/// Points a redeem's Impact Reaction script at `path` (renamed), leaving
/// whether it's on and its settings as they are.
fn set_reaction_script(config: &mut RedeemConfig, path: &str, name: &str) {
    if let Some(custom) = config.impact_reactions.as_mut().and_then(|v| v.get_mut("custom_script")).and_then(|c| c.as_object_mut()) {
        custom.insert("path".to_string(), serde_json::json!(path));
        custom.insert("name".to_string(), serde_json::json!(name));
    }
}

/// Opens `path` in the app Windows associates with it, or Notepad.
fn open_in_editor(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        #[link(name = "shell32")]
        unsafe extern "system" {
            fn ShellExecuteW(hwnd: *mut std::ffi::c_void, op: *const u16, file: *const u16, params: *const u16, dir: *const u16, show: i32) -> isize;
        }
        let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
        let (op, file) = (wide("open"), wide(&path.to_string_lossy()));
        let result = unsafe { ShellExecuteW(std::ptr::null_mut(), op.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), 1) };
        // 32 or less: failed (no app for .lua / .rhai, say)
        if result <= 32 {
            let _ = std::process::Command::new("notepad.exe").arg(path).spawn();
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}

fn show_in_folder(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("explorer.exe").raw_arg(format!("/select,\"{}\"", path.display())).spawn();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = std::process::Command::new("xdg-open").arg(path.parent().unwrap_or(path)).spawn();
    }
}

/// Where a new script of `kind` goes (your folders: the shipped ones may not
/// be writable), and its first version.
fn new_script(kind: Kind) -> Result<PathBuf, String> {
    let (dir, stem, ext) = match kind {
        Kind::Reaction => (crate::paths::user_filters_dir(), "new_reaction", "lua"),
        Kind::Redeem => (crate::paths::user_scripts_dir(), "new_redeem", "lua"),
        Kind::Filter => (crate::paths::user_filters_dir(), "new_filter", "rhai"),
    };
    let path = (1..1000)
        .map(|n| dir.join(if n == 1 { format!("{}.{}", stem, ext) } else { format!("{}_{}.{}", stem, n, ext) }))
        .find(|p| !p.exists())
        .ok_or("no free name")?;
    let name = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    let text = match kind {
        Kind::Reaction => REACTION_TEMPLATE.replace("{name}", &name),
        Kind::Redeem => REDEEM_TEMPLATE.replace("{name}", &name),
        Kind::Filter => FILTER_TEMPLATE.replace("{name}", &name),
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path)
}

const REACTION_TEMPLATE: &str = r#"-- {name}.lua — a Theater reaction: pick it as a redeem's Impact Reaction and
-- Theater runs this shader on the target when it's hit.
--
-- Theater sets `time` (seconds since the hit) and `duration` (the reaction's
-- Duration, s), plus each setting below (get_defaults: the reaction's sliders
-- in the Studio). It can also pass where the target was hit: see
-- bite_vanish.lua (hit_count, hits[], view_width/height, target_width/height).

obs = obslua

local shader_code = [[
uniform float4x4 ViewProj;
uniform texture2d image;

uniform float time;
uniform float duration;
uniform float strength;

sampler_state textureSampler {
    Filter    = Linear;
    AddressU  = Border;
    AddressV  = Border;
    BorderColor = 00000000;
};

struct VertData {
    float4 pos : POSITION;
    float2 uv  : TEXCOORD0;
};

VertData VSDefault(VertData v_in) {
    VertData vert_out;
    vert_out.pos = mul(float4(v_in.pos.xyz, 1.0), ViewProj);
    vert_out.uv  = v_in.uv;
    return vert_out;
}

float4 PSDefault(VertData v_in) : TARGET {
    float4 color = image.Sample(textureSampler, v_in.uv);
    // A red flash that fades out over the reaction
    float fade = 1.0 - saturate(time / max(duration, 0.001));
    color.rgb = lerp(color.rgb, float3(1.0, 0.2, 0.2) * color.a, strength * fade);
    return color;
}

technique Draw {
    pass {
        vertex_shader = VSDefault(v_in);
        pixel_shader  = PSDefault(v_in);
    }
}
]]

-- Its settings (Theater reads these defaults as the reaction's sliders)
function get_defaults(settings)
    obs.obs_data_set_default_double(settings, "strength", 0.6)
end
"#;

const REDEEM_TEMPLATE: &str = r#"-- {name}.lua — a custom redeem script: pick it as a redeem's Custom Lua
-- Script, and Theater runs on_redeem(params) each time the redeem fires.
--
-- params: target, user, trigger, wildcards, and each slider below (-- @param
-- name default min max label: the redeem's sliders in the Studio). Theater's
-- API is trust.* (trigger_reaction, spawn_throwable, set_source_transform,
-- play_sound, sleep…): see doom.lua.

-- @param intensity 1 0 5 Intensity

function on_redeem(params)
    local intensity = tonumber(params.intensity) or 1
    trust.trigger_reaction({
        target = params.target,
        impactReactions = {
            jolt = { enabled = true, intensity = 0.2 * intensity, duration_ms = 400 },
        },
    })
end
"#;

const FILTER_TEMPLATE: &str = r#"// {name}.rhai — an OBS filter written in Rhai, hosted by Theater Scene.
// After creating it, restart OBS once (filters are registered when OBS
// starts); from then on it reloads by itself each time you save it.
//
// `this` is the filter's own state: this.settings, this.enabled,
// this.width / this.height (its source's size), this.canvas_width /
// this.canvas_height, this.time (seconds), plus anything you store in it.
// tick returns the shader's values each frame, or () to draw the source
// untouched. Filters can share values with share(key, value) /
// shared_value(key). theatre_360_master.rhai / theatre_360_layer.rhai show
// the rest: update(), publish(), source_list(), remote(), SIZE = "canvas".

const ID = "{name}";
const NAME = "{name}";

const SHADER = `
uniform float4x4 ViewProj;
uniform texture2d image;

uniform float strength;
uniform float pulse;

sampler_state textureSampler {
    Filter    = Linear;
    AddressU  = Clamp;
    AddressV  = Clamp;
};

struct VertData {
    float4 pos : POSITION;
    float2 uv  : TEXCOORD0;
};

VertData VSDefault(VertData v_in) {
    VertData vert_out;
    vert_out.pos = mul(float4(v_in.pos.xyz, 1.0), ViewProj);
    vert_out.uv  = v_in.uv;
    return vert_out;
}

float4 PSDefault(VertData v_in) : TARGET {
    float4 color = image.Sample(textureSampler, v_in.uv);
    // A tint that breathes
    color.rgb = lerp(color.rgb, float3(0.4, 0.6, 1.0) * color.a, strength * pulse);
    return color;
}

technique Draw {
    pass {
        vertex_shader = VSDefault(v_in);
        pixel_shader  = PSDefault(v_in);
    }
}
`;

fn defaults() {
    #{ strength: 0.5, speed: 1.0 }
}

fn properties() {
    [
        slider("strength", "Strength", 0.0, 1.0, 0.01),
        slider("speed", "Speed", 0.0, 5.0, 0.05),
    ]
}

fn tick(dt) {
    #{ strength: this.settings.strength, pulse: (this.time * this.settings.speed * 3.0).sin() * 0.5 + 0.5 }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_templates_work() {
        let dir = std::env::temp_dir().join(format!("trust_templates_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let reaction = dir.join("t_reaction.lua");
        std::fs::write(&reaction, REACTION_TEMPLATE.replace("{name}", "t_reaction")).unwrap();
        let redeem = dir.join("t_redeem.lua");
        std::fs::write(&redeem, REDEEM_TEMPLATE.replace("{name}", "t_redeem")).unwrap();
        let filter = dir.join("t_filter.rhai");
        std::fs::write(&filter, FILTER_TEMPLATE.replace("{name}", "t_filter")).unwrap();

        let r = crate::script_catalog::read_entry(&reaction).unwrap();
        assert_eq!((r.kinds.clone(), r.problem.clone()), (vec![Kind::Reaction], None));
        assert_eq!(r.reaction_settings, [("strength".to_string(), 0.6)]);
        let d = crate::script_catalog::read_entry(&redeem).unwrap();
        assert_eq!((d.kinds.clone(), d.problem.clone(), d.threats.len()), (vec![Kind::Redeem], None, 0));
        assert_eq!(d.sliders[0].name, "intensity");
        let f = crate::script_catalog::read_entry(&filter).unwrap();
        assert_eq!((f.kinds.clone(), f.problem.clone(), f.filter_id.clone()), (vec![Kind::Filter], None, Some("t_filter".to_string())));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_rename_keeps_the_folder_and_extension_and_refuses_bad_names() {
        let dir = std::env::temp_dir().join(format!("trust_rename_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let old = dir.join("doom.lua");
        std::fs::write(&old, "-- x").unwrap();
        std::fs::write(dir.join("taken.lua"), "-- y").unwrap();
        assert_eq!(renamed_path(&old, "  doomsday "), Ok(dir.join("doomsday.lua")));
        assert_eq!(renamed_path(&old, "doomsday.LUA"), Ok(dir.join("doomsday.lua")));
        assert_eq!(renamed_path(&old, "Doom"), Ok(dir.join("Doom.lua")), "other capitals only");
        assert!(renamed_path(&old, "doom").unwrap_err().contains("already"));
        assert!(renamed_path(&old, "taken").unwrap_err().contains("there already"));
        assert!(renamed_path(&old, "   ").unwrap_err().contains("empty"));
        assert!(renamed_path(&old, "../up").is_err());
        assert!(renamed_path(&old, "a:b").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_renamed_reaction_script_is_followed_by_its_redeems_files() {
        let dir = std::env::temp_dir().join(format!("trust_rename_redeem_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut config = RedeemConfig::default();
        config.set_custom_script("C:/old/bite.lua", "bite");
        config.custom_redeem_script = Some("C:/old/doom.lua".to_string());
        let json = serde_json::to_string_pretty(&config).unwrap();
        let file = dir.join("r.json");
        std::fs::write(&file, &json).unwrap();
        std::fs::write(dir.join("r.rdm"), &json).unwrap();

        rewrite_redeem_file(&file, |c| set_reaction_script(c, "C:/new/chomp.lua", "chomp")).unwrap();
        for twin in ["r.json", "r.rdm"] {
            let saved: RedeemConfig = serde_json::from_str(&std::fs::read_to_string(dir.join(twin)).unwrap()).unwrap();
            assert_eq!(saved.get_custom_script_path().as_deref(), Some("C:/new/chomp.lua"), "{}", twin);
            assert!(saved.get_custom_script_enabled(), "still on");
            assert_eq!(saved.custom_redeem_script.as_deref(), Some("C:/old/doom.lua"), "the rest as it was");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
