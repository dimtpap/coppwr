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

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use ashpd::desktop::settings::{ColorScheme, Settings};
use futures_util::{
    future::{self, AbortHandle},
    stream::{self, StreamExt as _},
};

pub struct SystemThemeListener {
    handle: AbortHandle,
    running: Arc<AtomicBool>,
}

impl SystemThemeListener {
    pub fn new(ctx: &egui::Context) -> Self {
        let ctx = ctx.clone();

        let (fut, handle) = future::abortable(async move {
            let settings = Settings::new().await?;

            // No notification is received for the already set scheme
            let initial = settings.color_scheme().await?;

            let incoming = settings.receive_color_scheme_changed().await?;

            let mut stream = stream::once(std::future::ready(initial)).chain(incoming);
            while let Some(cs) = stream.next().await {
                match cs {
                    ColorScheme::PreferDark => {
                        ctx.options_mut(|o| o.fallback_theme = egui::Theme::Dark);
                    }
                    ColorScheme::PreferLight => {
                        ctx.options_mut(|o| o.fallback_theme = egui::Theme::Light);
                    }
                    _ => {}
                }
            }

            Ok::<_, ashpd::Error>(())
        });

        let running = Arc::new(AtomicBool::new(true));

        std::thread::spawn({
            let running = Arc::clone(&running);
            move || {
                if let Ok(Err(e)) = pollster::block_on(fut) {
                    eprintln!("Error while waiting for system theme change: {e}");
                }
                running.store(false, Ordering::Relaxed);
            }
        });

        Self { handle, running }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

impl Drop for SystemThemeListener {
    fn drop(&mut self) {
        self.handle.abort();
    }
}
