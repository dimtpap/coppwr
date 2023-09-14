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

use std::collections::BTreeMap;

use eframe::egui;

pub fn key_val_table(
    ui: &mut egui::Ui,
    min_scrolled_height: f32,
    max_height: f32,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    egui::ScrollArea::vertical()
        .min_scrolled_height(min_scrolled_height)
        .max_height(max_height)
        .show(ui, |ui| {
            egui::Grid::new("kvtable")
                .num_columns(2)
                .max_col_width(ui.available_width())
                .striped(true)
                .show(ui, add_contents);
        });
}

pub fn key_val_display(
    ui: &mut egui::Ui,
    min_scrolled_height: f32,
    max_height: f32,
    header: &str,
    kv: impl Iterator<Item = (impl Into<egui::WidgetText>, impl Into<egui::WidgetText>)>,
) {
    egui::CollapsingHeader::new(header).show(ui, |ui| {
        key_val_table(ui, min_scrolled_height, max_height, |ui| {
            for (k, v) in kv {
                ui.label(k);
                ui.label(v);
                ui.end_row();
            }
        });
    });
}

pub fn map_editor(
    ui: &mut egui::Ui,
    min_scrolled_height: f32,
    max_height: f32,
    map: &mut BTreeMap<String, String>,
    user_additions: &mut EditableKVList,
) {
    key_val_table(ui, min_scrolled_height, max_height, |ui| {
        map.retain(|k, v| {
            ui.label(k);
            let keep = ui
                .with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    let keep = !ui.button("Delete").clicked();
                    egui::TextEdit::singleline(v)
                        .hint_text("Value")
                        .desired_width(f32::INFINITY)
                        .show(ui);
                    keep
                })
                .inner;
            ui.end_row();
            keep
        });
    });

    ui.separator();

    ui.label("Add items");
    user_additions.show(ui);
}

#[derive(Default)]
pub struct EditableKVList {
    list: Vec<(String, String)>,
}

impl EditableKVList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
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

#[derive(Default)]
pub struct MapEditor {
    properties: BTreeMap<String, String>,
    user_additions: EditableKVList,
}

impl MapEditor {
    pub fn set_map(&mut self, map: BTreeMap<String, String>) {
        self.properties = map;
    }

    pub fn show(&mut self, ui: &mut egui::Ui, min_scrolled_height: f32, max_height: f32) {
        map_editor(
            ui,
            min_scrolled_height,
            max_height,
            &mut self.properties,
            &mut self.user_additions,
        );
    }

    pub fn take(&mut self) -> BTreeMap<String, String> {
        self.properties.extend(self.user_additions.take());

        std::mem::take(&mut self.properties)
    }
}
