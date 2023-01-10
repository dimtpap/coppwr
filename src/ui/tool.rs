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

use eframe::egui;

use crate::pipewire_backend::PipeWireRequest;

pub(super) trait Tool {
    fn draw(&mut self, ui: &mut egui::Ui, rsx: &pipewire::channel::Sender<PipeWireRequest>);
}

pub(super) struct WindowedTool<'a, T: Tool> {
    pub open: bool,
    title: &'a str,
    pub tool: T,
}

impl<'a, T: Tool> WindowedTool<'a, T> {
    pub fn new(title: &'a str, tool: T) -> Self {
        WindowedTool {
            open: false,
            title,
            tool,
        }
    }

    pub fn window(
        &mut self,
        ctx: &egui::Context,
        rsx: &pipewire::channel::Sender<PipeWireRequest>,
    ) {
        egui::Window::new(self.title)
            .vscroll(true)
            .open(&mut self.open)
            .show(ctx, |ui| {
                self.tool.draw(ui, rsx);
            });
    }
}
