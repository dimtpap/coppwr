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
mod profiler_deserialize;
mod ui;

use crate::backend::Request;
use crate::ui::CoppwrApp;

fn main() {
    pipewire::init();

    let (pt, erx, rsx) = crate::backend::run();

    if let Err(e) = eframe::run_native(
        env!("CARGO_PKG_NAME"),
        eframe::NativeOptions {
            app_id: Some(String::from("xyz.dimtpap.coppwr")),
            icon_data: eframe::IconData::try_from_png_bytes(
                &include_bytes!("../assets/icon/256.png")[..],
            )
            .ok(),
            ..Default::default()
        },
        Box::new({
            let rsx = rsx.clone();
            |_| Box::new(CoppwrApp::new(erx, rsx))
        }),
    ) {
        eprintln!("Failed to start the GUI: {e}");
        rsx.send(Request::Stop).ok();
    }

    if let Err(e) = pt.join() {
        eprintln!("The PipeWire thread has paniced: {e:?}");
    }

    unsafe {
        pipewire::deinit();
    }
}
