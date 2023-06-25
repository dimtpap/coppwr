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

use std::ops::Not;

use eframe::egui;

use crate::backend::Request;
use crate::ui::{common::EditableKVList, Tool};

#[derive(Default)]
pub struct ModuleLoader {
    module_dir: String,
    name: String,
    args: String,
    props: EditableKVList,
}

impl Tool for ModuleLoader {
    const NAME: &'static str = "Module Loader";

    fn draw(&mut self, ui: &mut egui::Ui, sx: &pipewire::channel::Sender<Request>) {
        self.draw(ui, sx);
    }
}

impl ModuleLoader {
    fn draw(&mut self, ui: &mut egui::Ui, sx: &pipewire::channel::Sender<Request>) {
        ui.add(
            egui::TextEdit::singleline(&mut self.module_dir)
                .hint_text("Modules directory (Leave empty for default)")
                .desired_width(f32::INFINITY),
        )
        .on_hover_text("The path of the directory where the module can be found");
        ui.add(
            egui::TextEdit::singleline(&mut self.name)
                .hint_text("Name")
                .desired_width(f32::INFINITY),
        );
        ui.add(
            egui::TextEdit::multiline(&mut self.args)
                .hint_text("Arguments")
                .desired_width(f32::INFINITY),
        );

        ui.separator();

        ui.label("Properties");

        self.props.draw(ui);

        ui.separator();

        ui.horizontal(|ui| {
            ui.add_enabled_ui(!self.name.is_empty(), |ui| {
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
                        name: self.name.clone(),
                        args: self.args.is_empty().not().then(|| self.args.clone()),
                        props: self
                            .props
                            .list()
                            .is_empty()
                            .not()
                            .then(|| self.props.list().clone()),
                    })
                    .ok();
                }
            });
            if ui.button("Clear").clicked() {
                self.name.clear();
                self.args.clear();
                self.props.clear();
            }
        });
    }
}
