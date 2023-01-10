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

use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::{Rc, Weak},
};

use eframe::egui;
use pipewire as pw;
use pw::{permissions::Permissions, registry::Permission, types::ObjectType};

use crate::pipewire_backend::{ObjectMethod, PipeWireRequest};

fn key_val_table(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::ScrollArea::vertical()
        .min_scrolled_height(400f32)
        .show(ui, |ui| {
            egui::Grid::new("kvtable")
                .num_columns(2)
                .striped(true)
                .show(ui, add_contents);
        });
}

fn key_val_display<'a>(
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

/// Object type specific data
pub(super) enum ObjectData {
    Client {
        permissions: Option<Vec<Permissions>>,
        new_property: String,
    },
    Other(ObjectType),
}

impl From<ObjectType> for ObjectData {
    fn from(value: ObjectType) -> Self {
        match value {
            ObjectType::Client => Self::Client {
                permissions: None,
                new_property: String::new(),
            },
            t => Self::Other(t),
        }
    }
}

impl ObjectData {
    fn pipewire_type(&self) -> &ObjectType {
        match self {
            Self::Client { .. } => &ObjectType::Client,
            Self::Other(t) => t,
        }
    }

    fn draw_data(
        &mut self,
        ui: &mut egui::Ui,
        rsx: &pw::channel::Sender<PipeWireRequest>,
        id: u32,
    ) {
        match self {
            Self::Client { permissions, .. } => {
                egui::CollapsingHeader::new("Permissions").show(ui, |ui| {
                    if ui.small_button("Get permissions").clicked() {
                        rsx.send(PipeWireRequest::CallObjectMethod(
                            id,
                            ObjectMethod::ClientGetPermissions {
                                index: 0,
                                num: u32::MAX,
                            },
                        ))
                        .ok();
                    }

                    let Some(permissions) = permissions else {
                            return;
                    };

                    ui.group(|ui| {
                        permissions.retain_mut(|p| {
                            let mut keep = true;

                            ui.horizontal(|ui| {
                                ui.label("ID");
                                ui.add(egui::widgets::DragValue::new(&mut p.id));

                                for (permission, label) in [
                                    (Permission::R, "Read"),
                                    (Permission::W, "Write"),
                                    (Permission::X, "Execute"),
                                    (Permission::M, "Metadata"),
                                ] {
                                    if ui
                                        .selectable_label(p.permissions.contains(permission), label)
                                        .clicked()
                                    {
                                        p.permissions.toggle(permission);
                                    }
                                }

                                keep = !ui.small_button("Delete").clicked();
                            });

                            keep
                        });

                        ui.separator();

                        if ui.button("Add permission").clicked() {
                            permissions.push(Permissions {
                                id: 0,
                                permissions: Permission::empty(),
                            });
                        }
                    });

                    if ui.small_button("Update permissions").clicked() {
                        rsx.send(PipeWireRequest::CallObjectMethod(
                            id,
                            ObjectMethod::ClientUpdatePermissions(permissions.clone()),
                        ))
                        .ok();
                    }
                });
            }
            Self::Other(_) => {}
        }
    }
}

/// A PipeWire object
pub(super) struct Global {
    id: u32,
    name: Option<String>,
    parent: Option<u32>,

    subobjects: Vec<Weak<RefCell<Global>>>,

    info: Option<Box<[(&'static str, String)]>>,
    props: BTreeMap<String, String>,

    object: ObjectData,
}

impl Global {
    pub fn new(
        id: u32,
        object_type: pw::types::ObjectType,
        props: Option<BTreeMap<String, String>>,
    ) -> Self {
        let mut this = Self {
            id,
            name: None,
            parent: None,
            subobjects: Vec::new(),
            info: None,
            props: props.unwrap_or_default(),
            object: ObjectData::from(object_type),
        };

        if !this.props().is_empty() {
            this.update();
        }

        this
    }

    fn update(&mut self) {
        self.parent = 'find_parent_id: {
            let keys = match self.object_type() {
                ObjectType::Node => {
                    if self.props().contains_key("device.id") {
                        ["device.id"].as_slice()
                    } else {
                        ["client.id"].as_slice()
                    }
                }
                ObjectType::Port => ["node.id"].as_slice(),
                ObjectType::Link => ["link.output.port", "link.input.port"].as_slice(),
                _ => break 'find_parent_id None,
            };

            for k in keys {
                if let Some(parent_id) = self.props().get(*k).and_then(|v| v.parse::<u32>().ok()) {
                    break 'find_parent_id Some(parent_id);
                }
            }

            None
        };

        let mut name = 'name: {
            match self.object_type() {
                t @ (ObjectType::Device | ObjectType::Node) => {
                    let lookups = match t {
                        ObjectType::Device => ["device.nick", "device.description", "device.name"],
                        ObjectType::Node => ["node.nick", "node.description", "node.name"],
                        _ => {
                            unreachable!();
                        }
                    };
                    for l in lookups {
                        if let Some(n) = self.props.get(l) {
                            break 'name Some(n);
                        }
                    }
                    None
                }
                ObjectType::Port => self.props.get("port.name"),
                ObjectType::Core => self.props.get("core.name"),
                _ => None,
            }
        };

        if name.is_none() {
            for (k, v) in self.props.iter().filter(|(k, _)| k.contains(".name")) {
                if k == "library.name"
                    || k == "factory.name" && *self.object_type() != ObjectType::Factory
                {
                    continue;
                }
                name = Some(v);
                break;
            }
        }

        self.name = name.cloned();
    }

    pub fn draw(
        &mut self,
        ui: &mut egui::Ui,
        draw_subobjects: bool,
        searched_property: &str,
        rsx: &pw::channel::Sender<PipeWireRequest>,
    ) {
        ui.group(|ui| {
            ui.set_width(ui.available_width());

            ui.vertical(|ui| {
                if let Some(name) = self.name() {
                    ui.label(name);
                }

                ui.horizontal(|ui| {
                    ui.label(self.id.to_string());
                    ui.label(self.object_type().to_str());
                });

                if ui.small_button("Destroy").clicked() {
                    rsx.send(PipeWireRequest::DestroyObject(self.id)).ok();
                }

                ui.push_id(self.id, |ui| {
                    if let Some(info) = self.info() {
                        key_val_display(ui, "Info", info.iter().map(|(k, v)| (*k, v.as_str())));
                    }

                    if !searched_property.is_empty() {
                        if let Some(val) = self.props().get(searched_property) {
                            ui.horizontal(|ui| {
                                ui.label(searched_property);
                                ui.label(val);
                            });
                        }
                    }

                    // Clients can have their properties updated
                    if let ObjectData::Client {
                        new_property: ref mut new_property_key,
                        ..
                    } = self.object
                    {
                        egui::CollapsingHeader::new("Properties").show(ui, |ui| {
                            key_val_table(ui, |ui| {
                                self.props.retain(|k, v| {
                                    let mut keep = true;
                                    ui.label(k);
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Min),
                                        |ui| {
                                            keep = !ui.button("Delete").clicked();
                                            egui::TextEdit::singleline(v)
                                                .hint_text("Value")
                                                .desired_width(f32::INFINITY)
                                                .show(ui);
                                        },
                                    );
                                    ui.end_row();
                                    keep
                                });
                            });
                            ui.add_space(5.);

                            ui.horizontal(|ui| {
                                egui::TextEdit::singleline(new_property_key)
                                    .hint_text("Property key")
                                    .show(ui);
                                if ui.button("Add").clicked() {
                                    self.props
                                        .insert(std::mem::take(new_property_key), String::new());
                                }
                            });

                            if ui.button("Update properties").clicked() {
                                rsx.send(PipeWireRequest::CallObjectMethod(
                                    self.id,
                                    ObjectMethod::ClientUpdateProperties(self.props.clone()),
                                ))
                                .ok();
                            }
                        });
                    } else {
                        key_val_display(
                            ui,
                            "Properties",
                            self.props().iter().map(|(k, v)| (k.as_str(), v.as_str())),
                        );
                    }

                    let subobjects_header = match self.object_type() {
                        ObjectType::Device | ObjectType::Client => "Nodes",
                        ObjectType::Node => "Ports",
                        ObjectType::Port => "Links",
                        _ => {
                            return;
                        }
                    };

                    if !self.subobjects.is_empty() {
                        self.subobjects.retain(|sub| sub.upgrade().is_some());

                        egui::CollapsingHeader::new(subobjects_header).show(ui, |ui| {
                            let subobjects = self.subobjects.iter().filter_map(|sub| sub.upgrade());
                            if draw_subobjects {
                                match self.object_type() {
                                    ObjectType::Device | ObjectType::Client => {
                                        for sub in subobjects {
                                            sub.borrow_mut().draw(ui, true, searched_property, rsx);
                                        }
                                    }
                                    ObjectType::Node => {
                                        let mut outs = Vec::with_capacity(self.subobjects.len());
                                        let mut ins = Vec::with_capacity(self.subobjects.len());
                                        let mut unk = Vec::with_capacity(self.subobjects.len());

                                        for port in subobjects {
                                            match port
                                                .borrow()
                                                .props
                                                .get("port.direction")
                                                .map(String::as_str)
                                            {
                                                Some("in") => ins.push(Rc::clone(&port)),
                                                Some("out") => outs.push(Rc::clone(&port)),
                                                _ => unk.push(Rc::clone(&port)),
                                            }
                                        }

                                        for (label, ports) in [
                                            ("Outputs", outs),
                                            ("Inputs", ins),
                                            ("Unknown direction", unk),
                                        ] {
                                            if ports.is_empty() {
                                                continue;
                                            }
                                            ui.label(label);
                                            ui.columns(ports.len(), |ui| {
                                                for (i, port) in ports.into_iter().enumerate() {
                                                    port.borrow_mut().draw(
                                                        &mut ui[i],
                                                        true,
                                                        searched_property,
                                                        rsx,
                                                    );
                                                }
                                            });
                                        }
                                    }
                                    ObjectType::Port => {
                                        ui.columns(self.subobjects.len(), |ui| {
                                            for (i, sub) in subobjects.enumerate() {
                                                sub.borrow_mut().draw(
                                                    &mut ui[i],
                                                    true,
                                                    searched_property,
                                                    rsx,
                                                );
                                            }
                                        });
                                    }
                                    _ => {}
                                }
                            } else {
                                for sub in subobjects {
                                    ui.label(sub.borrow().id.to_string());
                                }
                            }
                        });
                    }

                    self.object.draw_data(ui, rsx, self.id);
                });
            });
        });
    }

    pub fn name(&self) -> Option<&String> {
        self.name.as_ref()
    }

    pub fn object_type(&self) -> &pw::types::ObjectType {
        self.object.pipewire_type()
    }

    pub fn add_subobject(&mut self, subobject: Weak<RefCell<Global>>) {
        self.subobjects.push(subobject);
    }

    pub fn props(&self) -> &BTreeMap<String, String> {
        &self.props
    }

    pub fn set_props(&mut self, props: BTreeMap<String, String>) {
        self.props = props;
        self.update();
    }

    pub fn info(&self) -> Option<&[(&'static str, String)]> {
        self.info.as_deref()
    }

    pub fn set_info(&mut self, info: Option<Box<[(&'static str, String)]>>) {
        self.info = info;
    }

    pub fn object_mut(&mut self) -> &mut ObjectData {
        &mut self.object
    }

    pub fn parent_id(&self) -> Option<u32> {
        self.parent
    }
}
