// Copyright 2023-2025 Dimitris Papaioannou <dimtpap@protonmail.com>
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

#[cfg(feature = "xdg_desktop_portals")]
mod system_theme_listener;

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
        Box::new(|cc| {
            #[cfg(not(feature = "xdg_desktop_portals"))]
            // Explicitly set current theme to fallback theme
            // since system theme detection will not be available
            cc.egui_ctx.options_mut(|o| {
                if o.theme_preference == egui::ThemePreference::System {
                    o.theme_preference = o.fallback_theme.into()
                }
            });

            Ok(Box::new(CoppwrApp::new(cc)))
        }),
    ) {
        eprintln!("Failed to start the GUI: {e}");
    }

    unsafe {
        pipewire::deinit();
    }
}
