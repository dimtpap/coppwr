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

mod backend;
mod ui;

use log::error;

use crate::ui::CoppwrApp;

fn main() {
    pipewire::init();

    if let Err(e) = eframe::run_native(
        env!("CARGO_PKG_NAME"),
        eframe::NativeOptions {
            app_id: Some(String::from("xyz.dimtpap.coppwr")),
            icon_data: eframe::IconData::try_from_png_bytes(
                include_bytes!("../assets/icon/256.png").as_slice(),
            )
            .ok(),
            ..eframe::NativeOptions::default()
        },
        Box::new(|_| Box::new(CoppwrApp::new())),
    ) {
        error!("Failed to start the GUI: {e}");
    }

    unsafe {
        pipewire::deinit();
    }
}
