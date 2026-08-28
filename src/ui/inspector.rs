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

use std::rc::Rc;

use eframe::egui;

use pipewire::types::ObjectType;

use crate::{
    backend::{self, Event, RemoteInfo},
    ui::{
        ContextManager, GlobalsStore, Graph, MetadataEditor, ObjectCreator, Profiler,
        app::{Settings, View},
        globals_store::ObjectData,
        util::{persistence::PersistentView, tool::Windowed},
    },
};

/// Stores the persistent view states
#[derive(Default)]
#[cfg_attr(
    feature = "persistence",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct PersistentData {
    graph: Option<<Graph as PersistentView>::Data>,
}

/// Holds all of the UIs, and their states, for interacting with PipeWire.
/// It processes messages from the backend and modifies them accordingly.
pub struct Inspector {
    handle: backend::Handle,

    globals: GlobalsStore,
    profiler: Profiler,
    graph: Graph,

    object_creator: Windowed<ObjectCreator>,
    metadata_editor: Windowed<MetadataEditor>,
    context_manager: Windowed<ContextManager>,
}

impl Inspector {
    pub fn new(
        remote: RemoteInfo,
        mainloop_properties: Vec<(String, String)>,
        context_properties: Vec<(String, String)>,
        restore_data: Option<&PersistentData>,
    ) -> Self {
        Self {
            handle: backend::Handle::run(remote, mainloop_properties, context_properties),

            globals: GlobalsStore::new(),
            profiler: Profiler::with_max_profilings(250),
            graph: restore_data
                .and_then(|data| data.graph.as_ref())
                .map_or_else(Graph::new, Graph::with_data),

            object_creator: Windowed::default(),
            metadata_editor: Windowed::default(),
            context_manager: Windowed::default(),
        }
    }

    pub fn save_data(&self, data: &mut Option<PersistentData>) {
        let new_data = PersistentData {
            graph: self.graph.save_data(),
        };

        match data {
            Some(data) => {
                if let Some(graph) = new_data.graph {
                    data.graph = Some(graph);
                }
            }
            None => *data = Some(new_data),
        }
    }

    pub fn views_menu_buttons(
        &self,
        ui: &mut egui::Ui,
        dock_state: &mut egui_dock::DockState<View>,
    ) {
        let open_tabs = dock_state
            .iter_all_tabs()
            .fold(0, |acc, (_, &tab)| acc | tab as u8);

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
                (View::Graph, "🖧 Graph", "Visual representation of the graph"),
            ] {
                let open = open_tabs & tab as u8 != 0;

                ui.add_enabled_ui(!open, |ui| {
                    if ui
                        .selectable_label(open, title)
                        .on_hover_text(description)
                        .clicked()
                    {
                        dock_state.push_to_focused_leaf(tab);
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
                    &mut self.context_manager.open,
                    "🗄 Context Manager",
                    "Manage the PipeWire context",
                ),
            ] {
                ui.toggle_value(open, name).on_hover_text(description);
            }
        });
    }

    pub fn tool_windows(&mut self, ctx: &egui::Context) {
        self.object_creator.window(ctx, &self.handle.sx);
        self.metadata_editor.window(ctx, &self.handle.sx);
        self.context_manager.window(ctx, &self.handle.sx);
    }

    #[must_use = "Indicates whether the connection to the backend has ended"]
    pub fn process_events_or_stop(&mut self) -> bool {
        use std::sync::mpsc::TryRecvError;

        loop {
            match self.handle.rx().try_recv() {
                Ok(event) => {
                    if matches!(event, Event::Stop) {
                        return true;
                    }
                    self.process_event(event);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    eprintln!("Events sender has disconnected");
                    return true;
                }
            }
        }
        false
    }

    fn process_event(&mut self, e: Event) {
        match e {
            Event::GlobalAdded(id, object_type, props) => {
                let global = self.globals.add_global(id, object_type, props);
                let global_borrow = global.borrow();

                if global_borrow.props().is_empty() {
                    return;
                }

                match *global_borrow.object_type() {
                    ObjectType::Factory => {
                        self.object_creator.tool.add_factory(global);
                    }
                    ObjectType::Metadata => {
                        self.metadata_editor.tool.add_metadata(global);
                    }
                    _ => {}
                }
            }
            Event::GlobalRemoved(id) => {
                if let Some(removed) = self.globals.remove_global(id) {
                    match *removed.borrow().object_type() {
                        ObjectType::Metadata => {
                            self.metadata_editor.tool.remove_metadata(id);
                        }
                        ObjectType::Factory => {
                            self.object_creator.tool.remove_factory(id);
                        }
                        ObjectType::Node => {
                            self.graph.remove_node(id);
                        }
                        ObjectType::Port => {
                            if let Some(node_id) = removed.borrow().parent_id() {
                                self.graph.remove_port(node_id, id);
                            }
                        }
                        ObjectType::Link => {
                            self.graph.remove_link(id);
                        }
                        _ => {}
                    }
                }
            }
            Event::GlobalInfo(id, info, props) => {
                if let Some(props) = props {
                    self.globals.set_global_props(id, props);
                }

                let Some(global) = self.globals.get_global(id) else {
                    return;
                };

                // Add to graph
                {
                    let global_borrow = global.borrow();
                    match *global_borrow.object_type() {
                        ObjectType::Node => {
                            self.graph.add_node(global);
                        }
                        ObjectType::Port => {
                            if let Some(info) = &info {
                                match info[0].1.as_str() {
                                    "Input" => {
                                        self.graph.add_input_port(global);
                                    }
                                    "Output" => self.graph.add_output_port(global),
                                    _ => {}
                                }
                            }
                        }
                        ObjectType::Link => {
                            if let Some(info) = &info
                                && let (
                                    Some(output_node),
                                    Some(output_port),
                                    Some(input_node),
                                    Some(input_port),
                                ) = (
                                    info[2].1.parse().ok(),
                                    info[3].1.parse().ok(),
                                    info[0].1.parse().ok(),
                                    info[1].1.parse().ok(),
                                )
                            {
                                self.graph.add_link(
                                    output_node,
                                    output_port,
                                    input_node,
                                    input_port,
                                    id,
                                );
                            }
                        }

                        _ => {}
                    }
                }

                global.borrow_mut().set_info(info);
            }
            Event::PortMediaType { id, media_type } => {
                let Some(port) = self.globals.get_global(id) else {
                    return;
                };

                *port.borrow_mut().object_data_mut() = ObjectData::Port(media_type);
            }
            Event::ProfilerProfile(samples) => {
                self.profiler.add_profilings(samples);
            }
            Event::MetadataProperty {
                id,
                subject,
                key,
                type_,
                value,
            } => match (key, value) {
                (Some(key), Some(value)) => {
                    let Some(metadata) = self.globals.get_global(id) else {
                        return;
                    };
                    self.metadata_editor
                        .tool
                        .add_property(metadata, subject, key, type_, value);
                }
                (Some(key), None) => {
                    self.metadata_editor.tool.remove_property(id, &key);
                }
                (None, _) => {
                    self.metadata_editor.tool.clear_properties(id);
                }
            },
            Event::ClientPermissions(id, _, perms) => {
                if let Some(global) = self.globals.get_global(id)
                    && let ObjectData::Client { permissions, .. } =
                        global.borrow_mut().object_data_mut()
                {
                    *permissions = Some(perms);
                }
            }
            Event::ContextProperties(properties) => {
                self.context_manager.tool.set_context_properties(properties);
            }
            Event::Stop => unreachable!(),
        }
    }

    pub fn show_view(&mut self, ui: &mut egui::Ui, view: View, settings: &Settings) {
        match view {
            View::Profiler => {
                self.profiler
                    .show_profiler(ui, &self.handle.sx, settings.update_rate, |id| {
                        id.try_into()
                            .ok()
                            .and_then(|id| self.globals.get_global(id))
                            .map(Rc::downgrade)
                    });
            }
            View::ProcessViewer => {
                self.profiler.show_process_viewer(
                    ui,
                    &self.handle.sx,
                    settings.update_rate,
                    |id| {
                        id.try_into()
                            .ok()
                            .and_then(|id| self.globals.get_global(id))
                            .map(Rc::downgrade)
                    },
                );
            }
            View::GlobalTracker => {
                self.globals.show(ui, &self.handle.sx);
            }
            View::Graph => {
                self.graph.show(ui, &self.handle.sx);
            }
        }
    }
}
