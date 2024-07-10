// Copyright 2023-2024 Dimitris Papaioannou <dimtpap@protonmail.com>
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

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use eframe::egui;
use pipewire::types::ObjectType;

use crate::{
    backend::{self, Request},
    ui::{
        globals_store::Global,
        util::{
            tool::Tool,
            uis::{global_info_button, EditableKVList},
        },
    },
};

struct Factory {
    object_type: ObjectType,
    global: Rc<RefCell<Global>>,
}

impl Factory {
    fn name(&self) -> String {
        self.global.borrow().name().cloned().unwrap_or_default()
    }
}

#[derive(Default)]
pub struct ObjectCreator {
    factories: HashMap<u32, Factory>,
    selected_factory: Option<u32>,

    props: EditableKVList,
}

impl Tool for ObjectCreator {
    const NAME: &'static str = "Object Creator";

    fn show(&mut self, ui: &mut egui::Ui, sx: &backend::Sender) {
        self.show(ui, sx);
    }
}

impl ObjectCreator {
    pub fn add_factory(&mut self, global: &Rc<RefCell<Global>>) {
        let (id, object_type) = {
            let global = global.borrow();

            let object_type = global.props().get("factory.type.name").map(|object_type| {
                match object_type.as_str() {
                    "PipeWire:Interface:Link" => ObjectType::Link,
                    "PipeWire:Interface:Port" => ObjectType::Port,
                    "PipeWire:Interface:Node" => ObjectType::Node,
                    "PipeWire:Interface:Client" => ObjectType::Client,
                    "PipeWire:Interface:Device" => ObjectType::Device,
                    "PipeWire:Interface:Registry" => ObjectType::Registry,
                    "PipeWire:Interface:Profiler" => ObjectType::Profiler,
                    "PipeWire:Interface:Metadata" => ObjectType::Metadata,
                    "PipeWire:Interface:Factory" => ObjectType::Factory,
                    "PipeWire:Interface:Module" => ObjectType::Module,
                    "PipeWire:Interface:Core" => ObjectType::Core,
                    "PipeWire:Interface:Endpoint" => ObjectType::Endpoint,
                    "PipeWire:Interface:EndpointLink" => ObjectType::EndpointLink,
                    "PipeWire:Interface:EndpointStream" => ObjectType::EndpointStream,
                    "PipeWire:Interface:ClientSession" => ObjectType::ClientSession,
                    "PipeWire:Interface:ClientEndpoint" => ObjectType::ClientEndpoint,
                    "PipeWire:Interface:ClientNode" => ObjectType::ClientNode,
                    _ => ObjectType::Other(object_type.clone()),
                }
            });

            (global.id(), object_type)
        };

        if let Some(object_type) = object_type {
            self.factories.insert(
                id,
                Factory {
                    object_type,
                    global: Rc::clone(global),
                },
            );
        }
    }

    pub fn remove_factory(&mut self, id: u32) {
        self.factories.remove(&id);
    }

    fn show(&mut self, ui: &mut egui::Ui, sx: &backend::Sender) {
        let factory = if let Some(id) = self.selected_factory {
            let factory = self.factories.get(&id);
            if factory.is_none() {
                self.selected_factory = None;
            }
            factory
        } else {
            None
        };

        // Store the name here to avoid calling .borrow() every time it's needed
        let factory_name = factory.map(Factory::name).unwrap_or_default();

        ui.horizontal(|ui| {
            let cb = egui::ComboBox::from_label("Factory");
            let cb = if factory.is_some() {
                cb.selected_text(&factory_name)
            } else {
                cb.selected_text("No factory selected")
            };

            cb.show_ui(ui, |ui| {
                for (id, factory) in &self.factories {
                    ui.selectable_value(&mut self.selected_factory, Some(*id), factory.name());
                }
            });

            if let Some(global) = factory.map(|f| &f.global) {
                global_info_button(ui, Some(global), sx);
            }
        });

        if let Some(factory) = factory {
            ui.label(format!("Creates {}", factory.object_type.to_str()));
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
                        factory_name,
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
