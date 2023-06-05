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

use std::{sync::mpsc, time::Duration};

use eframe::egui;
use pipewire as pw;
use pw::types::ObjectType;

use super::{
    global::ObjectData, GlobalsStore, MetadataEditor, ModuleLoader, ObjectCreator, Profiler,
    WindowedTool,
};
use crate::backend::{PipeWireEvent, PipeWireRequest};

#[derive(Clone, Copy)]
enum View {
    GlobalTracker = 1 << 0,
    Profiler = 1 << 1,
    ProcessViewer = 1 << 2,
}

impl View {
    fn as_str(self) -> &'static str {
        match self {
            View::Profiler => "Profiler",
            View::ProcessViewer => "Process Viewer",
            View::GlobalTracker => "Global Tracker",
        }
    }
}

struct CoppwrViewer {
    open_tabs: u8,

    erx: mpsc::Receiver<PipeWireEvent>,
    rsx: pw::channel::Sender<PipeWireRequest>,

    globals: GlobalsStore,
    profiler: Profiler,

    object_creator: WindowedTool<'static, ObjectCreator>,
    metadata_editor: WindowedTool<'static, MetadataEditor>,
    module_loader: WindowedTool<'static, ModuleLoader>,
}

impl CoppwrViewer {
    pub fn new(
        erx: mpsc::Receiver<PipeWireEvent>,
        rsx: pw::channel::Sender<PipeWireRequest>,
    ) -> Self {
        Self {
            open_tabs: View::GlobalTracker as u8,

            erx,
            rsx,

            globals: GlobalsStore::new(),
            profiler: Profiler::with_max_profilings(250),

            object_creator: WindowedTool::new("Object Creator", ObjectCreator::new()),
            metadata_editor: WindowedTool::new("Metadata Editor", MetadataEditor::new()),
            module_loader: WindowedTool::new("Module Loader", ModuleLoader::new()),
        }
    }

    pub fn views_menu_buttons(&mut self, ui: &mut egui::Ui, tree: &mut egui_dock::Tree<View>) {
        ui.menu_button("View", |ui| {
            for (tab, title, description) in [
                (
                    View::GlobalTracker,
                    "📑 Global Tracker",
                    "List of all the objects in the remote",
                ),
                (View::Profiler, "📈 Profiler", "Graphs of profiling data"),
                (
                    View::ProcessViewer,
                    "⏱ Process Viewer",
                    "Performance measurements of running nodes",
                ),
            ] {
                let bit = tab as u8;
                ui.add_enabled_ui(self.open_tabs & bit == 0, |ui| {
                    if ui
                        .selectable_label(self.open_tabs & bit != 0, title)
                        .on_hover_text(description)
                        .clicked()
                    {
                        self.open_tabs |= bit;
                        tree.push_to_focused_leaf(tab);
                    }
                });
            }
        });
    }

    pub fn tools_menu_buttons(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Tools", |ui| {
            for (open, name, description) in [
                (
                    &mut self.object_creator.open,
                    "⛭ Object Creator",
                    "Create an object on the remote",
                ),
                (
                    &mut self.metadata_editor.open,
                    "🗐 Metadata Editor",
                    "Edit remote metadata",
                ),
                (
                    &mut self.module_loader.open,
                    "🗄 Module Loader",
                    "Load a module in the local context",
                ),
            ] {
                if ui
                    .selectable_label(*open, name)
                    .on_hover_text(description)
                    .clicked()
                {
                    *open = !*open;
                }
            }
        });
    }

    pub fn tool_windows(&mut self, ctx: &egui::Context) {
        self.object_creator.window(ctx, &self.rsx);
        self.metadata_editor.window(ctx, &self.rsx);
        self.module_loader.window(ctx, &self.rsx);
    }

    fn process_events(&mut self) {
        while let Ok(e) = self.erx.try_recv() {
            match e {
                PipeWireEvent::GlobalAdded(id, object_type, props) => {
                    let global = self.globals.add_global(id, object_type, props);

                    if global.props().is_empty() {
                        continue;
                    }

                    match *global.object_type() {
                        ObjectType::Factory => {
                            if let (Some(name), Some(object_type)) =
                                (global.name(), global.props().get("factory.type.name"))
                            {
                                let object_type = match object_type.as_str() {
                                    "PipeWire:Interface:Link" => ObjectType::Link,
                                    "PipeWire:Interface:Port" => ObjectType::Port,
                                    "PipeWire:Interface:Node" => ObjectType::Node,
                                    "PipeWire:Interface:Client" => ObjectType::Client,
                                    "PipeWire:Interface:Device" => ObjectType::Device,
                                    "PipeWire:Interface:Registry" => ObjectType::Registry,
                                    "PipeWire:Interface:Profiler" => ObjectType::Profiler,
                                    "PipeWire:Interface:Metadata" => ObjectType::Metadata,
                                    "PipeWire:Interface:Factory" => ObjectType::Factory,
                                    "PipeWire:Interface:Module" => ObjectType::Module,
                                    "PipeWire:Interface:Core" => ObjectType::Core,
                                    "PipeWire:Interface:Endpoint" => ObjectType::Endpoint,
                                    "PipeWire:Interface:EndpointLink" => ObjectType::EndpointLink,
                                    "PipeWire:Interface:EndpointStream" => {
                                        ObjectType::EndpointStream
                                    }
                                    "PipeWire:Interface:ClientSession" => ObjectType::ClientSession,
                                    "PipeWire:Interface:ClientEndpoint" => {
                                        ObjectType::ClientEndpoint
                                    }
                                    "PipeWire:Interface:ClientNode" => ObjectType::ClientNode,
                                    _ => ObjectType::Other(object_type.clone()),
                                };
                                self.object_creator.tool.add_factory(id, name, object_type);
                            }
                        }
                        ObjectType::Metadata => {
                            if let Some(name) = global.props().get("metadata.name") {
                                self.metadata_editor.tool.add_metadata(id, name);
                            }
                        }
                        _ => {}
                    }
                }
                PipeWireEvent::GlobalRemoved(id) => {
                    if let Some(removed) = self.globals.remove_global(id) {
                        match *removed.borrow().object_type() {
                            ObjectType::Metadata => {
                                self.metadata_editor.tool.remove_metadata(id);
                            }
                            ObjectType::Factory => {
                                self.object_creator.tool.remove_factory(id);
                            }
                            _ => {}
                        }
                    }
                }
                PipeWireEvent::GlobalInfo(id, info) => {
                    self.globals.set_global_info(id, Some(info));
                }
                PipeWireEvent::GlobalProperties(id, props) => {
                    self.globals.set_global_props(id, props);
                }
                PipeWireEvent::ProfilerProfile(samples) => {
                    self.profiler.add_profilings(samples);
                }
                PipeWireEvent::MetadataProperty {
                    id,
                    subject,
                    key,
                    type_,
                    value,
                } => match key {
                    Some(key) => match value {
                        Some(value) => {
                            let Some(metadata) = self.globals.get_global(id) else {
                                return;
                            };
                            self.metadata_editor.tool.add_property(
                                id,
                                metadata
                                    .borrow()
                                    .name()
                                    .cloned()
                                    .unwrap_or_else(|| format!("Unnamed metadata {id}")),
                                subject,
                                key,
                                type_,
                                value,
                            );
                        }
                        None => {
                            self.metadata_editor.tool.remove_property(id, &key);
                        }
                    },
                    None => {
                        self.metadata_editor.tool.clear_properties(id);
                    }
                },
                PipeWireEvent::ClientPermissions(id, _, perms) => {
                    if let Some(global) = self.globals.get_global(id) {
                        if let ObjectData::Client { permissions, .. } =
                            global.borrow_mut().object_mut()
                        {
                            *permissions = Some(perms);
                        }
                    }
                }
            }
        }
    }
}

impl egui_dock::TabViewer for CoppwrViewer {
    type Tab = View;

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match *tab {
            View::Profiler => {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.profiler.draw_profiler(ui);
                });
            }
            View::ProcessViewer => {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.profiler.draw_process_viewer(ui);
                });
            }
            View::GlobalTracker => {
                self.globals.draw(ui, &self.rsx);
            }
        }
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.as_str().into()
    }

    fn on_close(&mut self, tab: &mut Self::Tab) -> bool {
        self.open_tabs &= !(*tab as u8);
        true
    }
}

pub struct CoppwrApp {
    rsx: pw::channel::Sender<PipeWireRequest>,

    tree: egui_dock::Tree<View>,
    viewer: CoppwrViewer,

    about_opened: bool,
}

impl CoppwrApp {
    pub fn new(
        erx: mpsc::Receiver<PipeWireEvent>,
        rsx: pw::channel::Sender<PipeWireRequest>,
    ) -> Self {
        let mut tabs = Vec::with_capacity(3 /* Number of views */);
        tabs.push(View::GlobalTracker);

        Self {
            rsx: rsx.clone(),
            tree: egui_dock::Tree::new(tabs),
            viewer: CoppwrViewer::new(erx, rsx),
            about_opened: false,
        }
    }
}

impl eframe::App for CoppwrApp {
    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        if self.rsx.send(PipeWireRequest::Stop).is_err() {
            eprintln!("Error sending stop request to PipeWire");
        };
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("❌ Quit").clicked() {
                        frame.close();
                    }
                });

                self.viewer.views_menu_buttons(ui, &mut self.tree);
                self.viewer.tools_menu_buttons(ui);

                ui.menu_button("Help", |ui| {
                    if ui.button("❓ About").clicked() {
                        self.about_opened = true;
                    }
                })
            });
        });

        egui::Window::new("About")
			.collapsible(false)
			.fixed_size([350f32, 150f32])
			.default_pos([
				(frame.info().window_info.size.x - 350f32) / 2f32,
				(frame.info().window_info.size.y - 150f32) / 2f32,
			])
			.open(&mut self.about_opened)
			.show(ctx, |ui| {
				ui.vertical_centered(|ui| {
					ui.heading(env!("CARGO_PKG_NAME"));
					ui.label(env!("CARGO_PKG_VERSION"));
					ui.label(env!("CARGO_PKG_DESCRIPTION"));

					ui.separator();

					ui.label("2023 Dimitris Papaioannou");
					ui.hyperlink(env!("CARGO_PKG_REPOSITORY"));

					ui.separator();

					ui.label("This program is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License version 3 as published by the Free Software Foundation.");
				});
			});

        self.viewer.process_events();

        self.viewer.tool_windows(ctx);

        let mut style = egui_dock::Style::from_egui(ctx.style().as_ref());
        style.tabs.inner_margin = egui::Margin::symmetric(5., 5.);
        egui_dock::DockArea::new(&mut self.tree)
            .style(style)
            .show(ctx, &mut self.viewer);

        // egui won't update until there is interaction so data shown may be out of date
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}
