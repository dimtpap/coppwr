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

use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::{Rc, Weak},
    sync::LazyLock,
};

use eframe::egui;
use pipewire::{
    self as pw,
    permissions::{Permission, PermissionFlags},
    types::ObjectType,
};

use crate::{
    backend::{self, ObjectMethod, Request},
    ui::util::uis::{key_val_display, map_editor, EditableKVList},
};

fn draw_permissions(ui: &mut egui::Ui, p: &mut Permission) {
    static PERMISSIONS: LazyLock<&[(PermissionFlags, &'static str)]> = LazyLock::new(|| {
        #[cfg(feature = "pw_v0_3_77")]
        if crate::backend::remote_version().is_some_and(|ver| ver.0 > 0 || ver.2 >= 77) {
            return [
                (PermissionFlags::R, "Read"),
                (PermissionFlags::W, "Write"),
                (PermissionFlags::X, "Execute"),
                (PermissionFlags::M, "Metadata"),
                (PermissionFlags::L, "Link"),
            ]
            .as_slice();
        }

        [
            (PermissionFlags::R, "Read"),
            (PermissionFlags::W, "Write"),
            (PermissionFlags::X, "Execute"),
            (PermissionFlags::M, "Metadata"),
        ]
        .as_slice()
    });

    ui.label("ID");
    ui.add(egui::widgets::DragValue::from_get_set(|v| {
        if let Some(v) = v {
            p.set_id(v as _);
        }
        p.id() as _
    }));

    for &(permission, label) in *PERMISSIONS {
        if ui
            .selectable_label(p.permission_flags().contains(permission), label)
            .clicked()
        {
            let mut flags = p.permission_flags();
            flags.toggle(permission);

            p.set_permission_flags(flags);
        }
    }
}

/// Object type specific data
pub enum ObjectData {
    Client {
        permissions: Option<Vec<Permission>>,
        user_permissions: Vec<Permission>,
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
                            user_permissions.push(Permission::new(0, PermissionFlags::empty()));
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

        let mut name = {
            match self.object_type() {
                t @ (ObjectType::Device | ObjectType::Node) => {
                    let lookups = match t {
                        ObjectType::Device => ["device.nick", "device.description", "device.name"],
                        ObjectType::Node => ["node.nick", "node.description", "node.name"],
                        _ => {
                            unreachable!();
                        }
                    };
                    lookups
                        .into_iter()
                        .find_map(|lookup| self.props.get(lookup))
                }
                ObjectType::Port => self.props.get("port.name"),
                ObjectType::Core => self.props.get("core.name"),
                ObjectType::Factory => self.props.get("factory.name"),
                _ => None,
            }
        };

        if name.is_none() {
            for (k, v) in self
                .props
                .iter()
                .filter(|(k, _)| k.ends_with(".name") && k.as_str() != "library.name")
            {
                if *self.object_type() != ObjectType::Factory && k == "factory.name" {
                    continue;
                }
                name = Some(v);
                break;
            }
        }

        self.name = name.cloned();
    }

    pub fn show(&mut self, ui: &mut egui::Ui, draw_subobjects: bool, sx: &backend::Sender) {
        fn subobjects_display(
            ui: &mut egui::Ui,
            id_salt: Option<&str>,
            len: usize,
            subobjects: impl Iterator<Item = Rc<RefCell<Global>>>,
            sx: &backend::Sender,
        ) {
            let width = ui.available_width() / len as f32 - 6.;

            let sc = egui::ScrollArea::horizontal();

            if let Some(id_salt) = id_salt {
                sc.id_salt(id_salt)
            } else {
                sc
            }
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for sub in subobjects {
                        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                            ui.set_max_width(width);
                            sub.borrow_mut().show(ui, true, sx);
                        });
                    }
                });
            });
        }

        ui.group(|ui| {
            if ui.layout().cross_justify {
                // Frames don't expand unless the children do
                ui.set_width(ui.available_width());
            }

            ui.scope(|ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

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
            });

            ui.push_id(self.id, |ui| {
                if let Some(info) = self.info() {
                    key_val_display(ui, 400f32, f32::INFINITY, "Info", info.iter().cloned());
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
                            self.props.extend(std::mem::take(&mut user_properties.list));

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
                        let subobjects = self.subobjects.iter().filter_map(Weak::upgrade);
                        if draw_subobjects {
                            match self.object_type() {
                                ObjectType::Device | ObjectType::Client => {
                                    ui.with_layout(
                                        egui::Layout::top_down_justified(egui::Align::Min),
                                        |ui| {
                                            for sub in subobjects {
                                                sub.borrow_mut().show(ui, true, sx);
                                            }
                                        },
                                    );
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

                                        subobjects_display(
                                            ui,
                                            Some(label),
                                            ports.len(),
                                            ports.into_iter(),
                                            sx,
                                        );
                                    }
                                }
                                ObjectType::Port => {
                                    subobjects_display(
                                        ui,
                                        None,
                                        self.subobjects.len(),
                                        subobjects,
                                        sx,
                                    );
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

    pub const fn id(&self) -> u32 {
        self.id
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
