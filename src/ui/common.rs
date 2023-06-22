use eframe::egui;

#[derive(Default)]
pub struct EditableKVList {
    list: Vec<(String, String)>,
}

impl EditableKVList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn draw(&mut self, ui: &mut egui::Ui) {
        self.list.retain_mut(|(k, v)| {
            ui.horizontal(|ui| {
                let keep = !ui.button("Delete").clicked();
                ui.add(
                    egui::TextEdit::singleline(k)
                        .hint_text("Key")
                        .desired_width(ui.available_width() / 2.5),
                );
                ui.add(
                    egui::TextEdit::singleline(v)
                        .hint_text("Value")
                        .desired_width(ui.available_width()),
                );
                keep
            })
            .inner
        });

        if ui.button("Add").clicked() {
            self.list.push((String::new(), String::new()));
        }
    }

    pub fn list(&self) -> &Vec<(String, String)> {
        &self.list
    }

    pub fn take(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.list)
    }

    pub fn clear(&mut self) {
        self.list.clear();
    }
}
