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

mod backend;
mod ui;

use crate::ui::CoppwrApp;

fn main() {
    pipewire::init();

    if let Err(e) = eframe::run_native(
        env!("CARGO_PKG_NAME"),
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder {
                title: Some(env!("CARGO_PKG_NAME").to_owned()),
                app_id: Some(String::from("io.github.dimtpap.coppwr")),
                icon: eframe::icon_data::from_png_bytes(
                    include_bytes!("../assets/icon/256.png").as_slice(),
                )
                .ok()
                .map(std::sync::Arc::new),
                ..eframe::egui::ViewportBuilder::default()
            },
            ..eframe::NativeOptions::default()
        },
        {
            #[cfg(not(feature = "persistence"))]
            {
                Box::new(|_| Ok(Box::new(CoppwrApp::new())))
            }

            #[cfg(feature = "persistence")]
            {
                Box::new(|cc| Ok(Box::new(CoppwrApp::new(cc.storage))))
            }
        },
    ) {
        eprintln!("Failed to start the GUI: {e}");
    }

    unsafe {
        pipewire::deinit();
    }
}
