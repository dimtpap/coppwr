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

use std::collections::HashMap;

use eframe::egui;
use pipewire::types::ObjectType;

use crate::{
    backend::Request,
    ui::{common::EditableKVList, Tool},
};

struct Factory {
    name: String,
    object_type: ObjectType,
}

#[derive(Default)]
pub struct ObjectCreator {
    factories: HashMap<u32, Factory>,
    selected_factory: Option<u32>,

    props: EditableKVList,
}

impl Tool for ObjectCreator {
    const NAME: &'static str = "Object Creator";

    fn show(&mut self, ui: &mut egui::Ui, sx: &pipewire::channel::Sender<Request>) {
        self.show(ui, sx);
    }
}

impl ObjectCreator {
    pub fn add_factory(&mut self, id: u32, name: &str, object_type: ObjectType) {
        self.factories.insert(
            id,
            Factory {
                name: name.to_string(),
                object_type,
            },
        );
    }

    pub fn remove_factory(&mut self, id: u32) {
        self.factories.remove(&id);
    }

    fn show(&mut self, ui: &mut egui::Ui, sx: &pipewire::channel::Sender<Request>) {
        let factory = if let Some(id) = self.selected_factory {
            let factory = self.factories.get(&id);
            if factory.is_none() {
                self.selected_factory = None;
            }
            factory
        } else {
            None
        };

        let cb = egui::ComboBox::from_label("Factory");
        let cb = if let Some(factory) = factory {
            cb.selected_text(&factory.name)
        } else {
            cb.selected_text("No factory selected")
        };

        cb.show_ui(ui, |ui| {
            for (id, factory) in &self.factories {
                ui.selectable_value(&mut self.selected_factory, Some(*id), &factory.name);
            }
        });

        if let Some(factory) = factory {
            ui.horizontal(|ui| {
                ui.label("Creates ");
                ui.label(factory.object_type.to_str());
            });
        }

        ui.separator();

        ui.label("Properties");

        self.props.show(ui);

        ui.separator();

        ui.horizontal(|ui| {
            ui.add_enabled_ui(factory.is_some(), |ui| {
                if ui
                    .button("Create")
                    .on_disabled_hover_text("Select a factory first")
                    .clicked()
                {
                    let factory = factory.unwrap();
                    sx.send(Request::CreateObject(
                        factory.object_type.clone(),
                        factory.name.clone(),
                        self.props.list().clone(),
                    ))
                    .ok();
                }
            });
            if ui.button("Clear").clicked() {
                self.selected_factory = None;
                self.props.clear();
            }
        });
    }
}
