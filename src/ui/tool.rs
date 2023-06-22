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

use crate::backend::Request;

pub trait Tool {
    const NAME: &'static str;

    fn draw(&mut self, ui: &mut egui::Ui, sx: &pipewire::channel::Sender<Request>);
}

pub struct WindowedTool<T: Tool> {
    pub open: bool,
    pub tool: T,
}

impl<T: Tool> WindowedTool<T> {
    pub fn new(tool: T) -> Self {
        WindowedTool { open: false, tool }
    }

    pub fn window(&mut self, ctx: &egui::Context, sx: &pipewire::channel::Sender<Request>) {
        egui::Window::new(T::NAME)
            .vscroll(true)
            .open(&mut self.open)
            .show(ctx, |ui| {
                self.tool.draw(ui, sx);
            });
    }
}
