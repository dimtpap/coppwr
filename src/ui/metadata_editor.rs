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

use std::collections::{btree_map::Entry, BTreeMap};

use eframe::egui;

use crate::{
    backend::{self, ObjectMethod, Request},
    ui::Tool,
};

struct Property {
    subject: u32,
    type_: Option<String>,
    value: String,
}

impl Property {
    fn set_request(&self, key: String) -> ObjectMethod {
        ObjectMethod::MetadataSetProperty {
            subject: self.subject,
            key,
            type_: self.type_.as_ref().cloned(),
            value: Some(self.value.clone()),
        }
    }

    fn clear_request(&self, key: String) -> ObjectMethod {
        ObjectMethod::MetadataSetProperty {
            subject: self.subject,
            key,
            type_: self.type_.as_ref().cloned(),
            value: None,
        }
    }
}

struct Metadata {
    name: String,
    properties: BTreeMap<String, Property>,
    user_properties: Vec<(String, Property)>,
}

#[derive(Default)]
pub struct MetadataEditor {
    metadatas: BTreeMap<u32, Metadata>,
}

impl Tool for MetadataEditor {
    const NAME: &'static str = "Metadata Editor";

    fn show(&mut self, ui: &mut egui::Ui, sx: &backend::Sender) {
        self.show(ui, sx);
    }
}

impl MetadataEditor {
    pub fn add_metadata(&mut self, id: u32, name: &str) {
        self.metadatas.entry(id).or_insert(Metadata {
            name: name.to_owned(),
            properties: BTreeMap::new(),
            user_properties: Vec::new(),
        });
    }

    pub fn add_property(
        &mut self,
        id: u32,
        name: String,
        subject: u32,
        key: String,
        type_: Option<String>,
        value: String,
    ) {
        let prop = Property {
            subject,
            type_,
            value,
        };
        match self.metadatas.entry(id) {
            Entry::Occupied(e) => {
                let properties = &mut e.into_mut().properties;
                properties.insert(key, prop);
            }
            Entry::Vacant(e) => {
                let metadata = Metadata {
                    name,
                    properties: BTreeMap::new(),
                    user_properties: Vec::new(),
                };
                e.insert(metadata).properties.insert(key, prop);
            }
        }
    }

    pub fn remove_metadata(&mut self, id: u32) {
        self.metadatas.remove(&id);
    }

    pub fn remove_property(&mut self, id: u32, key: &str) {
        self.metadatas.entry(id).and_modify(|m| {
            m.properties.remove(key);
        });
    }

    pub fn clear_properties(&mut self, id: u32) {
        self.metadatas.entry(id).and_modify(|m| {
            m.properties.clear();
        });
    }

    fn show(&mut self, ui: &mut egui::Ui, sx: &backend::Sender) {
        for (id, metadata) in &mut self.metadatas {
            ui.group(|ui| {
                ui.heading(&metadata.name);
                ui.horizontal(|ui| {
                    ui.label(format!("ID: {id}"));
                    if ui.small_button("Clear").clicked() {
                        sx.send(Request::CallObjectMethod(*id, ObjectMethod::MetadataClear))
                            .ok();
                    }
                });
                egui::Grid::new(&metadata.name)
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        for (key, prop) in &mut metadata.properties {
                            ui.label(key);

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                if ui.small_button("Clear").clicked() {
                                    sx.send(Request::CallObjectMethod(
                                        *id,
                                        prop.clear_request(key.clone()),
                                    ))
                                    .ok();
                                }
                                if ui.small_button("Set").clicked() {
                                    sx.send(Request::CallObjectMethod(
                                        *id,
                                        prop.set_request(key.clone()),
                                    ))
                                    .ok();
                                }
                                let input = ui.add(
                                    egui::TextEdit::singleline(&mut prop.value)
                                        .hint_text("Value")
                                        .desired_width(f32::INFINITY),
                                );
                                if let Some(type_) = prop.type_.as_ref() {
                                    input.on_hover_text(format!(
                                        "Type: {type_}\nSubject: {}",
                                        prop.subject
                                    ));
                                } else {
                                    input.on_hover_text(format!("Subject: {}", prop.subject));
                                }
                            });

                            ui.end_row();
                        }
                    });

                ui.separator();

                egui::CollapsingHeader::new("Add properites")
                    .id_source(*id)
                    .show(ui, |ui| {
                        metadata.user_properties.retain_mut(|(key, prop)| {
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(key)
                                        .hint_text("Key")
                                        .desired_width(ui.available_width() / 2.),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut prop.value)
                                        .hint_text("Value")
                                        .desired_width(f32::INFINITY),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label("Subject");
                                ui.add(egui::widgets::DragValue::new(&mut prop.subject));

                                if ui.checkbox(&mut prop.type_.is_some(), "Type").changed() {
                                    if prop.type_.is_none() {
                                        prop.type_ = Some(String::new());
                                    } else {
                                        prop.type_ = None;
                                    }
                                }
                                if let Some(ref mut type_) = prop.type_ {
                                    ui.add(
                                        egui::TextEdit::singleline(type_)
                                            .hint_text("Type")
                                            .desired_width(f32::INFINITY),
                                    );
                                }
                            });
                            let keep = ui
                                .horizontal(|ui| {
                                    if ui.button("Set").clicked() {
                                        sx.send(Request::CallObjectMethod(
                                            *id,
                                            prop.set_request(key.clone()),
                                        ))
                                        .ok();
                                    }
                                    !ui.button("Delete").clicked()
                                })
                                .inner;

                            ui.separator();

                            keep
                        });

                        ui.horizontal(|ui| {
                            if ui.button("Add").clicked() {
                                metadata.user_properties.push((
                                    String::new(),
                                    Property {
                                        subject: 0,
                                        type_: None,
                                        value: String::new(),
                                    },
                                ));
                            }

                            ui.add_enabled_ui(!metadata.user_properties.is_empty(), |ui| {
                                if ui.button("Clear").clicked() {
                                    metadata.user_properties.clear();
                                }
                            });
                        });

                        ui.add_enabled_ui(!metadata.user_properties.is_empty(), |ui| {
                            if ui.button("Set all").clicked() {
                                for (key, prop) in std::mem::take(&mut metadata.user_properties) {
                                    sx.send(Request::CallObjectMethod(*id, prop.set_request(key)))
                                        .ok();
                                }
                            }
                        });
                    });
            });
        }
    }
}
