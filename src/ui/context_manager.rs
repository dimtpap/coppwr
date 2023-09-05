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

use std::{collections::BTreeMap, ops::Not};

use eframe::egui;

use crate::backend::Request;

use super::{
    common::{EditableKVList, PropertiesEditor},
    Tool,
};

#[derive(PartialEq, Eq)]
enum View {
    PropertiesEditor,
    ModuleLoader,
}

impl View {
    fn as_str(&self) -> &'static str {
        match self {
            Self::PropertiesEditor => "Properties editor",
            Self::ModuleLoader => "Module loader",
        }
    }
}

impl Default for View {
    fn default() -> Self {
        Self::PropertiesEditor
    }
}

#[derive(Default)]
pub struct ContextManager {
    view: View,

    properties: PropertiesEditor,

    module_dir: String,
    module_name: String,
    module_args: String,
    module_props: EditableKVList,
}

impl Tool for ContextManager {
    const NAME: &'static str = "Context Manager";

    fn show(&mut self, ui: &mut egui::Ui, sx: &pipewire::channel::Sender<Request>) {
        self.show(ui, sx);
    }
}

impl ContextManager {
    pub fn set_context_properties(&mut self, properties: BTreeMap<String, String>) {
        self.properties.set_properties(properties);
    }

    fn show(&mut self, ui: &mut egui::Ui, sx: &pipewire::channel::Sender<Request>) {
        egui::ComboBox::new("view", "View")
            .selected_text(self.view.as_str())
            .show_ui(ui, |ui| {
                for view in [View::PropertiesEditor, View::ModuleLoader] {
                    if ui
                        .selectable_label(self.view == view, view.as_str())
                        .clicked()
                    {
                        self.view = view;
                    }
                }
            });

        ui.separator();

        match self.view {
            View::PropertiesEditor => {
                self.properties.show(ui, 0f32, 250f32);

                ui.separator();

                ui.horizontal(|ui| {
                    if ui.small_button("Get properties").clicked() {
                        sx.send(Request::GetContextProperties).ok();
                    }

                    if ui.small_button("Update properties").clicked() {
                        sx.send(Request::UpdateContextProperties(
                            self.properties.take_as_map(),
                        ))
                        .ok();

                        sx.send(Request::GetContextProperties).ok();
                    }
                });
            }
            View::ModuleLoader => {
                ui.add(
                    egui::TextEdit::singleline(&mut self.module_dir)
                        .hint_text("Modules directory (Leave empty for default)")
                        .desired_width(f32::INFINITY),
                )
                .on_hover_text("The path of the directory where the module can be found");
                ui.add(
                    egui::TextEdit::singleline(&mut self.module_name)
                        .hint_text("Name")
                        .desired_width(f32::INFINITY),
                );
                ui.add(
                    egui::TextEdit::multiline(&mut self.module_args)
                        .hint_text("Arguments")
                        .desired_width(f32::INFINITY),
                );

                ui.separator();

                ui.label("Properties");

                self.module_props.show(ui);

                ui.separator();

                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!self.module_name.is_empty(), |ui| {
                        if ui
                            .button("Load")
                            .on_disabled_hover_text("Provide a module name first")
                            .clicked()
                        {
                            sx.send(Request::LoadModule {
                                module_dir: self
                                    .module_dir
                                    .is_empty()
                                    .not()
                                    .then(|| self.module_dir.clone()),
                                name: self.module_name.clone(),
                                args: self
                                    .module_args
                                    .is_empty()
                                    .not()
                                    .then(|| self.module_args.clone()),
                                props: self
                                    .module_props
                                    .list()
                                    .is_empty()
                                    .not()
                                    .then(|| self.module_props.list().clone()),
                            })
                            .ok();
                        }
                    });
                    if ui.button("Clear").clicked() {
                        self.module_dir.clear();
                        self.module_name.clear();
                        self.module_args.clear();
                        self.module_props.clear();
                    }
                });
            }
        }
    }
}
