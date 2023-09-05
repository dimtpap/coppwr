// Copyright 2023 Dimitris Papaioannou <dimtpap@protonmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 3 as published by
// the Free Software Foundation.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: GPL-3.0-only

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

    pub const fn list(&self) -> &Vec<(String, String)> {
        &self.list
    }

    pub fn list_mut(&mut self) -> &mut Vec<(String, String)> {
        &mut self.list
    }

    pub fn take(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.list)
    }

    pub fn clear(&mut self) {
        self.list.clear();
    }
}

pub fn key_val_table(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::ScrollArea::vertical()
        .min_scrolled_height(400f32)
        .show(ui, |ui| {
            egui::Grid::new("kvtable")
                .num_columns(2)
                .striped(true)
                .show(ui, add_contents);
        });
}

pub fn key_val_display<'a>(
    ui: &mut egui::Ui,
    header: &str,
    kv: impl Iterator<Item = (&'a str, &'a str)>,
) {
    egui::CollapsingHeader::new(header).show(ui, |ui| {
        key_val_table(ui, |ui| {
            for (k, v) in kv {
                ui.label(k);
                ui.label(v).on_hover_text(v);
                ui.end_row();
            }
        });
    });
}
