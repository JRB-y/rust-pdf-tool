//! The whole interface: a list of open files on the left, the action for the
//! selected file on the right.

use std::path::{Path, PathBuf};

use eframe::egui;
use pdftool::util::{Result, write_edit};
use pdftool::{merge, meta, pages};

#[derive(PartialEq)]
enum Tab {
    Metadata,
    Merge,
    Pages,
}

/// What the status line at the bottom shows after an action.
enum Status {
    Idle,
    Done(String),
    Failed(String),
}

#[derive(Default)]
struct Form {
    /// One text box per field of `meta::FIELDS`.
    values: Vec<String>,
    /// What the file held when it was read, to tell an edit from a removal.
    original: Vec<Option<String>>,
    /// Page count, version and dates, shown read-only.
    summary: String,
}

pub struct App {
    files: Vec<PathBuf>,
    selected: Option<usize>,
    tab: Tab,
    form: Form,
    page_spec: String,
    angle: i64,
    overwrite: bool,
    status: Status,
}

impl Default for App {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            selected: None,
            tab: Tab::Metadata,
            form: Form::default(),
            page_spec: String::new(),
            angle: 90,
            overwrite: false,
            status: Status::Idle,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.take_dropped_files(ctx);

        egui::TopBottomPanel::top("top").show(ctx, |ui| self.toolbar(ui));
        egui::TopBottomPanel::bottom("bottom").show(ctx, |ui| self.footer(ui));
        egui::SidePanel::left("files")
            .default_width(280.0)
            .show(ctx, |ui| self.file_list(ui));
        egui::CentralPanel::default().show(ctx, |ui| self.actions(ui));
    }
}

// ---------------------------------------------------------------- the panels

impl App {
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Ajouter des PDF…").clicked()
                && let Some(chosen) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_files()
            {
                self.add_files(chosen);
            }
            if ui.button("Vider la liste").clicked() {
                self.files.clear();
                self.select(None);
            }
            ui.separator();
            ui.selectable_value(&mut self.tab, Tab::Metadata, "Métadonnées");
            ui.selectable_value(&mut self.tab, Tab::Pages, "Pages");
            ui.selectable_value(&mut self.tab, Tab::Merge, "Fusionner");
        });
        ui.add_space(6.0);
    }

    fn file_list(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.heading("Fichiers");
        ui.label("Glissez-déposez vos PDF dans la fenêtre. L'ordre de la liste est l'ordre de fusion.");
        ui.add_space(6.0);

        let mut clicked = None;
        let mut move_up = None;
        let mut remove = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            for (index, file) in self.files.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(index > 0, egui::Button::new("↑").small())
                        .clicked()
                    {
                        move_up = Some(index);
                    }
                    if ui
                        .add_enabled(index + 1 < self.files.len(), egui::Button::new("↓").small())
                        .clicked()
                    {
                        move_up = Some(index + 1);
                    }
                    if ui.add(egui::Button::new("✕").small()).clicked() {
                        remove = Some(index);
                    }
                    let name = file.file_name().unwrap_or(file.as_os_str()).to_string_lossy();
                    if ui.selectable_label(self.selected == Some(index), name).clicked() {
                        clicked = Some(index);
                    }
                });
            }
        });

        if let Some(index) = move_up {
            self.files.swap(index - 1, index);
            self.select(Some(index - 1));
        }
        if let Some(index) = remove {
            self.files.remove(index);
            self.select(None);
        }
        if let Some(index) = clicked {
            self.select(Some(index));
        }
    }

    fn actions(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        match self.tab {
            Tab::Metadata => self.metadata_tab(ui),
            Tab::Pages => self.pages_tab(ui),
            Tab::Merge => self.merge_tab(ui),
        }
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.overwrite, "Remplacer le fichier d'origine")
                .on_hover_text(
                    "Décoché, chaque modification demande où enregistrer. \
                     Coché, le fichier est réécrit sur place (via un fichier temporaire).",
                );
            ui.separator();
            match &self.status {
                Status::Idle => ui.label("Prêt."),
                Status::Done(text) => ui.colored_label(egui::Color32::from_rgb(40, 150, 70), text),
                Status::Failed(text) => ui.colored_label(egui::Color32::from_rgb(190, 60, 50), text),
            };
        });
        ui.add_space(4.0);
    }

    // ------------------------------------------------------------- the tabs

    fn metadata_tab(&mut self, ui: &mut egui::Ui) {
        let Some(file) = self.selected_file() else {
            ui.label("Sélectionnez un fichier à gauche.");
            return;
        };

        ui.heading(file.file_name().unwrap_or_default().to_string_lossy());
        ui.label(&self.form.summary);
        ui.add_space(10.0);

        egui::Grid::new("metadata")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                for (field, value) in meta::FIELDS.iter().zip(&mut self.form.values) {
                    ui.label(label_of(field));
                    ui.add(egui::TextEdit::singleline(value).desired_width(420.0));
                    ui.end_row();
                }
            });

        ui.add_space(6.0);
        ui.label("Un champ vidé est supprimé du document.");
        ui.add_space(10.0);

        if ui.button("Enregistrer les métadonnées").clicked() {
            let changes: Vec<(&str, &String)> = meta::FIELDS
                .iter()
                .zip(&self.form.values)
                .filter(|(_, value)| !value.trim().is_empty())
                .map(|(field, value)| (*field, value))
                .collect();
            let cleared: Vec<String> = meta::FIELDS
                .iter()
                .zip(&self.form.values)
                .zip(&self.form.original)
                .filter(|((_, value), before)| value.trim().is_empty() && before.is_some())
                .map(|((field, _), _)| field.to_string())
                .collect();

            if changes.is_empty() && cleared.is_empty() {
                self.status = Status::Done("Rien à changer.".into());
                return;
            }

            let outcome = self.run(&file, "fichier-metadonnees.pdf", |target| {
                meta::update(&file, &changes, &cleared, target)
            });
            self.report(
                outcome,
                format!(
                    "{} champ(s) écrit(s), {} supprimé(s)",
                    changes.len(),
                    cleared.len()
                ),
            );
        }
    }

    fn pages_tab(&mut self, ui: &mut egui::Ui) {
        let Some(file) = self.selected_file() else {
            ui.label("Sélectionnez un fichier à gauche.");
            return;
        };

        ui.heading(file.file_name().unwrap_or_default().to_string_lossy());
        ui.label(&self.form.summary);
        ui.add_space(10.0);

        ui.horizontal(|ui| {
            ui.label("Pages :");
            ui.add(egui::TextEdit::singleline(&mut self.page_spec).desired_width(220.0));
            ui.label("par exemple 1-3,7 ou 4- (jusqu'à la fin)");
        });
        ui.add_space(10.0);

        ui.horizontal(|ui| {
            if ui.button("Garder ces pages").clicked() {
                let spec = self.page_spec.clone();
                let mut kept = 0;
                let outcome = self.run(&file, "pages-gardees.pdf", |target| {
                    kept = pages::keep(&file, &spec, target)?;
                    Ok(())
                });
                self.report(outcome, format!("{kept} page(s) gardée(s)"));
            }
            if ui.button("Supprimer ces pages").clicked() {
                let spec = self.page_spec.clone();
                let mut removed = 0;
                let outcome = self.run(&file, "pages-supprimees.pdf", |target| {
                    removed = pages::remove(&file, &spec, target)?;
                    Ok(())
                });
                self.report(outcome, format!("{removed} page(s) supprimée(s)"));
            }
        });

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(10.0);

        ui.horizontal(|ui| {
            ui.label("Rotation :");
            egui::ComboBox::from_id_salt("angle")
                .selected_text(format!("{}°", self.angle))
                .show_ui(ui, |ui| {
                    for angle in [90, 180, 270, -90] {
                        ui.selectable_value(&mut self.angle, angle, format!("{angle}°"));
                    }
                });
            if ui.button("Tourner").clicked() {
                let spec = self.page_spec.clone();
                let selection = (!spec.trim().is_empty()).then_some(spec);
                let angle = self.angle;
                let mut rotated = 0;
                let outcome = self.run(&file, "pages-tournees.pdf", |target| {
                    rotated = pages::rotate(&file, angle, selection.as_deref(), target)?;
                    Ok(())
                });
                self.report(outcome, format!("{rotated} page(s) tournée(s) de {angle}°"));
            }
            ui.label("Pages vides = tout le document.");
        });
    }

    fn merge_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Fusionner");
        ui.label("Les pages sont ajoutées dans l'ordre de la liste de gauche ; réordonnez avec ↑ et ↓.");
        ui.add_space(10.0);

        for (index, file) in self.files.iter().enumerate() {
            ui.label(format!("{}. {}", index + 1, file.display()));
        }
        ui.add_space(14.0);

        let enough = self.files.len() >= 2;
        if ui
            .add_enabled(enough, egui::Button::new("Fusionner dans un fichier…"))
            .clicked()
            && let Some(target) = save_dialog("fusion.pdf")
        {
            let files = self.files.clone();
            match merge::merge(&files, &target) {
                Ok(total) => {
                    self.status = Status::Done(format!("{} fichiers fusionnés, {total} pages", files.len()));
                    self.add_files(vec![target]);
                }
                Err(error) => self.status = Status::Failed(error.to_string()),
            }
        }
        if !enough {
            ui.label("Ajoutez au moins deux fichiers.");
        }
    }

    // ---------------------------------------------------------- the plumbing

    /// Run one edit: in place when "remplacer" is ticked, otherwise after
    /// asking where to save. Returns the file that was written.
    fn run(
        &self,
        file: &Path,
        suggested: &str,
        edit: impl FnOnce(&Path) -> Result<()>,
    ) -> Option<Result<PathBuf>> {
        let output = match self.overwrite {
            true => None,
            false => Some(save_dialog(suggested)?),
        };
        Some(write_edit(file, output, edit))
    }

    /// Turn the outcome of `run` into the status line, and keep any new file.
    fn report(&mut self, outcome: Option<Result<PathBuf>>, summary: String) {
        match outcome {
            None => {} // the save dialog was cancelled
            Some(Ok(written)) => {
                self.status = Status::Done(format!("{summary} → {}", written.display()));
                if !self.files.contains(&written) {
                    self.add_files(vec![written]);
                } else {
                    self.reload_form();
                }
            }
            Some(Err(error)) => self.status = Status::Failed(error.to_string()),
        }
    }

    fn take_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect()
        });
        if !dropped.is_empty() {
            self.add_files(dropped);
        }
    }

    fn add_files(&mut self, files: Vec<PathBuf>) {
        for file in files {
            if !self.files.contains(&file) {
                self.files.push(file);
            }
        }
        if !self.files.is_empty() {
            self.select(Some(self.files.len() - 1));
        }
    }

    fn selected_file(&self) -> Option<PathBuf> {
        self.files.get(self.selected?).cloned()
    }

    fn select(&mut self, index: Option<usize>) {
        self.selected = index.filter(|index| *index < self.files.len());
        self.reload_form();
    }

    /// Read the selected file's metadata into the form.
    fn reload_form(&mut self) {
        let Some(file) = self.selected_file() else {
            self.form = Form::default();
            return;
        };

        let meta = match meta::read(&file) {
            Ok(meta) => meta,
            Err(error) => {
                self.status = Status::Failed(error.to_string());
                self.form = Form::default();
                return;
            }
        };

        let found = [
            meta.title.clone(),
            meta.author.clone(),
            meta.subject.clone(),
            meta.keywords.clone(),
            meta.creator.clone(),
            meta.producer.clone(),
        ];
        self.form = Form {
            values: found
                .iter()
                .map(|value| value.clone().unwrap_or_default())
                .collect(),
            original: found.to_vec(),
            summary: summary_of(&meta),
        };
    }
}

// ---------------------------------------------------------------- small bits

fn label_of(field: &str) -> &'static str {
    match field {
        "title" => "Titre",
        "author" => "Auteur",
        "subject" => "Sujet",
        "keywords" => "Mots-clés",
        "creator" => "Créateur",
        _ => "Producteur",
    }
}

fn summary_of(meta: &lopdf::PdfMetadata) -> String {
    let mut summary = format!("{} page(s) · PDF {}", meta.page_count, meta.version);
    if let Some(created) = &meta.creation_date {
        summary += &format!(" · créé le {}", pdftool::util::pretty_date(created));
    }
    if let Some(modified) = &meta.modification_date {
        summary += &format!(" · modifié le {}", pdftool::util::pretty_date(modified));
    }
    if meta.encrypted {
        summary += " · chiffré";
    }
    summary
}

fn save_dialog(suggested: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("PDF", &["pdf"])
        .set_file_name(suggested)
        .save_file()
}
