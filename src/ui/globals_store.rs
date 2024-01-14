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

use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use eframe::egui;
use pipewire::types::ObjectType;

use crate::{backend, ui::util::uis::KvMatcher};

#[path = "global.rs"]
mod global;
pub use global::{Global, ObjectData};

pub struct GlobalsStore {
    globals: BTreeMap<u32, Rc<RefCell<Global>>>,

    group_subobjects: bool,

    shown_types: u16,
    properties_filter: KvMatcher,
}

const fn object_type_flag(t: &ObjectType) -> u16 {
    match t {
        ObjectType::Core => 1 << 0,
        ObjectType::Module => 1 << 1,
        ObjectType::Factory => 1 << 2,
        ObjectType::Device => 1 << 3,
        ObjectType::Client => 1 << 4,
        ObjectType::Node => 1 << 5,
        ObjectType::Port => 1 << 6,
        ObjectType::Link => 1 << 7,
        ObjectType::Metadata => 1 << 8,
        ObjectType::Profiler => 1 << 9,
        _ => 1 << 10,
    }
}

impl GlobalsStore {
    pub const fn new() -> Self {
        Self {
            globals: BTreeMap::new(),

            group_subobjects: true,
            shown_types: u16::MAX,

            properties_filter: KvMatcher::new(),
        }
    }

    pub fn add_global(
        &mut self,
        id: u32,
        object_type: ObjectType,
        props: Option<BTreeMap<String, String>>,
    ) -> &Rc<RefCell<Global>> {
        use std::collections::btree_map::Entry;

        let global = Rc::new(RefCell::new(Global::new(id, object_type, props)));

        // Add as subobject
        {
            let global_borrow = global.borrow();
            match *global_borrow.object_type() {
                ObjectType::Node | ObjectType::Port => {
                    if let Some(parent) = self.parent_of(&global_borrow) {
                        parent.borrow_mut().add_subobject(Rc::downgrade(&global));
                    }
                }
                ObjectType::Link => {
                    for port in [
                        global_borrow.props().get("link.input.port"),
                        global_borrow.props().get("link.output.port"),
                    ]
                    .into_iter()
                    .filter_map(|entry| entry.and_then(|id_str| id_str.parse().ok()))
                    .filter_map(|id| self.globals.get(&id))
                    {
                        port.borrow_mut().add_subobject(Rc::downgrade(&global));
                    }
                }
                _ => {}
            }
        }

        match self.globals.entry(id) {
            Entry::Occupied(mut e) => {
                e.insert(global);
                e.into_mut()
            }
            Entry::Vacant(e) => e.insert(global),
        }
    }

    pub fn get_global(&self, id: u32) -> Option<&Rc<RefCell<Global>>> {
        self.globals.get(&id)
    }

    pub fn remove_global(&mut self, id: u32) -> Option<Rc<RefCell<Global>>> {
        self.globals.remove(&id)
    }

    pub fn set_global_props(&mut self, id: u32, props: BTreeMap<String, String>) {
        self.globals
            .entry(id)
            .and_modify(|global| global.borrow_mut().set_props(props));
    }

    fn parent_of(&self, global: &Global) -> Option<&Rc<RefCell<Global>>> {
        global.parent_id().and_then(|id| self.globals.get(&id))
    }

    fn satisfies_filters(&self, global: &Global) -> bool {
        if self.group_subobjects {
            if let ObjectType::Node | ObjectType::Port = *global.object_type() {
                let mut parent = self.parent_of(global);
                while let Some(global) = parent.map(|g| g.borrow()) {
                    if self.satisfies_filters(&global) {
                        return false;
                    }
                    parent = self.parent_of(&global);
                }
            }
        }

        if self.shown_types & object_type_flag(global.object_type()) == 0 {
            return false;
        }

        if !self.properties_filter.matches(global.props().iter()) {
            return false;
        }

        true
    }

    pub fn show(&mut self, ui: &mut egui::Ui, sx: &backend::Sender) {
        ui.checkbox(&mut self.group_subobjects, "Group Subobjects")
                                .on_hover_text("Whether to group objects as parents/children (Client/Device > Nodes > Ports > Links) or show them separately");

        ui.collapsing("Filters", |ui| {
            ui.horizontal(|ui| {
                ui.label("Types");
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    for (t, text) in [
                        (ObjectType::Core, "Core"),
                        (ObjectType::Module, "Module"),
                        (ObjectType::Factory, "Factory"),
                        (ObjectType::Device, "Device"),
                        (ObjectType::Client, "Client"),
                        (ObjectType::Node, "Node"),
                        (ObjectType::Port, "Port"),
                        (ObjectType::Link, "Link"),
                        (ObjectType::Metadata, "Metadata"),
                        (ObjectType::Profiler, "Profiler"),
                        (ObjectType::Other(String::new()), "Others"),
                    ] {
                        if ui
                            .selectable_label(self.shown_types & object_type_flag(&t) != 0, text)
                            .clicked()
                        {
                            self.shown_types ^= object_type_flag(&t);
                        }
                    }

                    if ui.button("Toggle all").clicked() {
                        self.shown_types = u16::from(self.shown_types == 0) * u16::MAX;
                    }
                });
            });

            ui.separator();

            ui.label("Properties").on_hover_text(
                "Only globals with properties that match the below filters will be shown",
            );
            self.properties_filter.show(ui);
        });

        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                for mut global in self.globals.values().filter_map(|global| {
                    let global = global.borrow_mut();
                    self.satisfies_filters(&global).then_some(global)
                }) {
                    global.show(ui, self.group_subobjects, sx);
                }
            });
        });
    }
}
