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
    sync::OnceLock,
};

use eframe::egui;
use pipewire::{self as pw, permissions::Permissions, registry::Permission, types::ObjectType};

use crate::{
    backend::{self, ObjectMethod, Request},
    ui::common::{key_val_display, map_editor, EditableKVList},
};

fn draw_permissions(ui: &mut egui::Ui, p: &mut Permissions) {
    static PERMISSIONS: OnceLock<&[(Permission, &'static str)]> = OnceLock::new();

    ui.label("ID");
    ui.add(egui::widgets::DragValue::new(&mut p.id));

    for (permission, label) in PERMISSIONS
        .get_or_init(|| {
            #[cfg(feature = "pw_v0_3_77")]
            if crate::backend::remote_version().is_some_and(|ver| ver.2 >= 77) {
                return [
                    (Permission::R, "Read"),
                    (Permission::W, "Write"),
                    (Permission::X, "Execute"),
                    (Permission::M, "Metadata"),
                    (Permission::L, "Link"),
                ]
                .as_slice();
            }

            [
                (Permission::R, "Read"),
                (Permission::W, "Write"),
                (Permission::X, "Execute"),
                (Permission::M, "Metadata"),
            ]
            .as_slice()
        })
        .iter()
        .map(|(p, l)| (*p, *l))
    {
        if ui
            .selectable_label(p.permissions.contains(permission), label)
            .clicked()
        {
            p.permissions.toggle(permission);
        }
    }
}

/// Object type specific data
pub enum ObjectData {
    Client {
        permissions: Option<Vec<Permissions>>,
        user_permissions: Vec<Permissions>,
        user_properties: EditableKVList,
    },
    Other(ObjectType),
}

impl From<ObjectType> for ObjectData {
    fn from(value: ObjectType) -> Self {
        match value {
            ObjectType::Client => Self::Client {
                permissions: None,
                user_permissions: Vec::new(),
                user_properties: EditableKVList::new(),
            },
            t => Self::Other(t),
        }
    }
}

impl ObjectData {
    const fn pipewire_type(&self) -> &ObjectType {
        match self {
            Self::Client { .. } => &ObjectType::Client,
            Self::Other(t) => t,
        }
    }

    fn show(&mut self, ui: &mut egui::Ui, sx: &backend::Sender, id: u32) {
        match self {
            Self::Client {
                permissions,
                user_permissions,
                ..
            } => {
                ui.collapsing("Permissions", |ui| {
                    if ui.small_button("Get permissions").clicked() {
                        sx.send(Request::CallObjectMethod(
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
                        for p in permissions.iter_mut() {
                            ui.horizontal(|ui| {
                                draw_permissions(ui, p);
                            });
                        }

                        ui.separator();

                        ui.label("Add permissions");

                        user_permissions.retain_mut(|p| {
                            ui.horizontal(|ui| {
                                draw_permissions(ui, p);
                                !ui.small_button("Delete").clicked()
                            })
                            .inner
                        });

                        if ui.button("Add").clicked() {
                            user_permissions.push(Permissions {
                                id: 0,
                                permissions: Permission::empty(),
                            });
                        }
                    });

                    if ui.small_button("Update permissions").clicked() {
                        let mut all_permissions =
                            Vec::with_capacity(permissions.len() + user_permissions.len());

                        all_permissions.append(&mut permissions.clone());
                        all_permissions.append(user_permissions);

                        sx.send(Request::CallObjectMethod(
                            id,
                            ObjectMethod::ClientUpdatePermissions(all_permissions),
                        ))
                        .ok();

                        // Request the permissions instantly to update the UI
                        sx.send(Request::CallObjectMethod(
                            id,
                            ObjectMethod::ClientGetPermissions {
                                index: 0,
                                num: u32::MAX,
                            },
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
pub struct Global {
    id: u32,
    name: Option<String>,
    parent: Option<u32>,

    subobjects: Vec<Weak<RefCell<Global>>>,

    info: Option<Box<[(&'static str, String)]>>,
    props: BTreeMap<String, String>,

    object_data: ObjectData,
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
            object_data: ObjectData::from(object_type),
        };

        if !this.props().is_empty() {
            this.update();
        }

        this
    }

    fn update(&mut self) {
        self.parent = match self.object_type() {
            ObjectType::Node => self
                .props()
                .get("device.id")
                .or_else(|| self.props().get("client.id")),
            ObjectType::Port => self.props().get("node.id"),
            _ => None,
        }
        .and_then(|id| id.parse().ok());

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

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        draw_subobjects: bool,
        searched_property: &str,
        sx: &backend::Sender,
    ) {
        ui.group(|ui| {
            ui.set_width(ui.available_width());

            if let Some(name) = self.name() {
                ui.label(name);
            }

            ui.horizontal(|ui| {
                ui.label(self.id.to_string());
                ui.label(self.object_type().to_str());
            });

            ui.with_layout(egui::Layout::default(), |ui| {
                if ui.small_button("Destroy").clicked() {
                    sx.send(Request::DestroyObject(self.id)).ok();
                }
            });

            ui.push_id(self.id, |ui| {
                if let Some(info) = self.info() {
                    key_val_display(ui, 400f32, f32::INFINITY, "Info", info.iter().cloned());
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
                    ref mut user_properties,
                    ..
                } = self.object_data
                {
                    egui::CollapsingHeader::new("Properties").show(ui, |ui| {
                        map_editor(ui, 400f32, f32::INFINITY, &mut self.props, user_properties);

                        ui.separator();

                        if ui.button("Update properties").clicked() {
                            self.props.extend(user_properties.take());

                            sx.send(Request::CallObjectMethod(
                                self.id,
                                ObjectMethod::ClientUpdateProperties(self.props.clone()),
                            ))
                            .ok();
                        }
                    });
                } else {
                    key_val_display(ui, 400f32, f32::INFINITY, "Properties", self.props().iter());
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

                    ui.collapsing(subobjects_header, |ui| {
                        let subobjects = self.subobjects.iter().filter_map(std::rc::Weak::upgrade);
                        if draw_subobjects {
                            match self.object_type() {
                                ObjectType::Device | ObjectType::Client => {
                                    for sub in subobjects {
                                        sub.borrow_mut().show(ui, true, searched_property, sx);
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
                                                port.borrow_mut().show(
                                                    &mut ui[i],
                                                    true,
                                                    searched_property,
                                                    sx,
                                                );
                                            }
                                        });
                                    }
                                }
                                ObjectType::Port => {
                                    ui.columns(self.subobjects.len(), |ui| {
                                        for (i, sub) in subobjects.enumerate() {
                                            sub.borrow_mut().show(
                                                &mut ui[i],
                                                true,
                                                searched_property,
                                                sx,
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

                self.object_data.show(ui, sx, self.id);
            });
        });
    }

    pub const fn name(&self) -> Option<&String> {
        self.name.as_ref()
    }

    pub const fn object_type(&self) -> &pw::types::ObjectType {
        self.object_data.pipewire_type()
    }

    pub fn add_subobject(&mut self, subobject: Weak<RefCell<Self>>) {
        self.subobjects.push(subobject);
    }

    pub const fn props(&self) -> &BTreeMap<String, String> {
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

    pub fn object_data_mut(&mut self) -> &mut ObjectData {
        &mut self.object_data
    }

    pub const fn parent_id(&self) -> Option<u32> {
        self.parent
    }
}
