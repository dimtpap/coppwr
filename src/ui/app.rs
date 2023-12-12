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
use egui_dock::DockState;

#[cfg(feature = "xdg_desktop_portals")]
use ashpd::{desktop::screencast::SourceType, enumflags2::BitFlags};

use crate::backend::RemoteInfo;

use super::common::EditableKVList;

#[derive(Clone, Copy)]
#[cfg_attr(feature = "persistence", derive(serde::Serialize, serde::Deserialize))]
pub enum View {
    GlobalTracker = 1 << 0,
    Profiler = 1 << 1,
    ProcessViewer = 1 << 2,
    Graph = 1 << 3,
}

impl View {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Profiler => "Profiler",
            Self::ProcessViewer => "Process Viewer",
            Self::GlobalTracker => "Global Tracker",
            Self::Graph => "Graph",
        }
    }
}

mod inspector {
    use eframe::egui;

    use pipewire::types::ObjectType;

    use crate::{
        backend::{self, Event, RemoteInfo},
        ui::{
            globals_store::ObjectData, persistence::PersistentView, ContextManager, GlobalsStore,
            Graph, MetadataEditor, ObjectCreator, Profiler, WindowedTool,
        },
    };

    use super::View;

    #[cfg_attr(feature = "persistence", derive(serde::Serialize, serde::Deserialize))]
    pub struct ViewsData {
        graph: Option<<Graph as PersistentView>::Data>,
    }

    pub struct Inspector {
        handle: backend::Handle,

        globals: GlobalsStore,
        profiler: Profiler,
        graph: Graph,

        object_creator: WindowedTool<ObjectCreator>,
        metadata_editor: WindowedTool<MetadataEditor>,
        context_manager: WindowedTool<ContextManager>,
    }

    impl Inspector {
        pub fn new(
            remote: RemoteInfo,
            mainloop_properties: Vec<(String, String)>,
            context_properties: Vec<(String, String)>,
            views_data: Option<&ViewsData>,
        ) -> Self {
            Self {
                handle: backend::Handle::run(remote, mainloop_properties, context_properties),

                globals: GlobalsStore::new(),
                profiler: Profiler::with_max_profilings(250),
                graph: views_data
                    .and_then(|vd| vd.graph.as_ref())
                    .map_or_else(Graph::new, Graph::with_data),

                object_creator: WindowedTool::default(),
                metadata_editor: WindowedTool::default(),
                context_manager: WindowedTool::default(),
            }
        }

        pub fn save_data(&self, data: &mut Option<ViewsData>) {
            let new_data = ViewsData {
                graph: self.graph.save_data(),
            };

            match data {
                Some(ref mut data) => {
                    if let Some(graph) = new_data.graph {
                        data.graph = Some(graph)
                    }
                }
                None => *data = Some(new_data),
            }
        }

        pub fn views_menu_buttons(
            &mut self,
            ui: &mut egui::Ui,
            dock_state: &mut egui_dock::DockState<View>,
        ) {
            let open_tabs = dock_state
                .iter_nodes()
                .filter_map(|node| node.tabs())
                .flat_map(|tabs| tabs.iter())
                .fold(0, |acc, &tab| acc | tab as u8);

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
            while let Ok(e) = self.handle.rx.try_recv() {
                match e {
                    Event::Stop => return true,
                    e => self.process_event(e),
                }
            }

            false
        }

        fn process_event(&mut self, e: Event) {
            match e {
                Event::GlobalAdded(id, object_type, props) => {
                    let global = self.globals.add_global(id, object_type, props).borrow();

                    if global.props().is_empty() {
                        return;
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
                            if let Some(name) = global.name() {
                                self.metadata_editor.tool.add_metadata(id, name);
                            }
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
                            _ => {}
                        }
                    }
                    self.graph.remove_item(id);
                }
                Event::GlobalInfo(id, info) => {
                    let Some(global) = self.globals.get_global(id) else {
                        return;
                    };

                    // Add to graph
                    {
                        let global_borrow = global.borrow();
                        match *global_borrow.object_type() {
                            ObjectType::Node => {
                                self.graph.add_node(id, global);
                            }
                            ObjectType::Port => {
                                if let Some(parent) = global_borrow.parent_id() {
                                    let name = global_borrow.name().cloned().unwrap_or_default();
                                    match info[0].1.as_str() {
                                        "Input" => {
                                            self.graph.add_input_port(id, parent, name);
                                        }
                                        "Output" => self.graph.add_output_port(id, parent, name),
                                        _ => {}
                                    }
                                }
                            }
                            ObjectType::Link => {
                                if let Some((output, input)) =
                                    info[3].1.parse().ok().zip(info[1].1.parse().ok())
                                {
                                    self.graph.add_link(id, output, input);
                                }
                            }
                            _ => {}
                        }
                    }

                    global.borrow_mut().set_info(Some(info));
                }
                Event::GlobalProperties(id, props) => {
                    self.globals.set_global_props(id, props);
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
                Event::ClientPermissions(id, _, perms) => {
                    if let Some(global) = self.globals.get_global(id) {
                        if let ObjectData::Client { permissions, .. } =
                            global.borrow_mut().object_data_mut()
                        {
                            *permissions = Some(perms);
                        }
                    }
                }
                Event::ContextProperties(properties) => {
                    self.context_manager.tool.set_context_properties(properties);
                }
                Event::Stop => unreachable!(),
            }
        }
    }

    impl egui_dock::TabViewer for Inspector {
        type Tab = View;

        fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
            match *tab {
                View::Profiler => {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.profiler.show_profiler(ui);
                    });
                }
                View::ProcessViewer => {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.profiler.show_process_viewer(ui);
                    });
                }
                View::GlobalTracker => {
                    self.globals.show(ui, &self.handle.sx);
                }
                View::Graph => {
                    self.graph.show(ui, &mut self.handle.sx);
                }
            }
        }

        fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
            tab.as_str().into()
        }

        fn on_close(&mut self, _tab: &mut Self::Tab) -> bool {
            true
        }

        fn scroll_bars(&self, _tab: &Self::Tab) -> [bool; 2] {
            [false, false]
        }
    }
}

use inspector::{Inspector, ViewsData};

enum State {
    Connected {
        inspector: Inspector,
        about: bool,
    },
    Unconnected {
        remote: RemoteInfo,
        mainloop_properties: EditableKVList,
        context_properties: EditableKVList,
    },
}

impl State {
    pub fn unconnected_from_env() -> Self {
        let mut context_properties = EditableKVList::new();
        context_properties
            .list_mut()
            .push(("media.category".to_owned(), "Manager".to_owned()));

        Self::Unconnected {
            remote: RemoteInfo::default(),
            mainloop_properties: EditableKVList::new(),
            context_properties,
        }
    }

    pub fn new_connected(
        remote: RemoteInfo,
        mainloop_properties: Vec<(String, String)>,
        context_properties: Vec<(String, String)>,
        inspector_data: Option<&ViewsData>,
    ) -> Self {
        Self::Connected {
            inspector: Inspector::new(
                remote,
                mainloop_properties,
                context_properties,
                inspector_data,
            ),
            about: false,
        }
    }

    pub fn connect(&mut self, inspector_data: Option<&ViewsData>) {
        if let Self::Unconnected {
            remote,
            mainloop_properties,
            context_properties,
        } = self
        {
            *self = Self::new_connected(
                std::mem::take(remote),
                mainloop_properties.take(),
                context_properties.take(),
                inspector_data,
            );
        }
    }

    pub fn disconnect(&mut self) {
        if let Self::Connected { .. } = self {
            *self = Self::unconnected_from_env();
        }
    }

    fn save_inspector_data(&self, data: &mut Option<ViewsData>) {
        if let Self::Connected { inspector, .. } = self {
            inspector.save_data(data);
        }
    }
}

#[cfg(feature = "persistence")]
mod storage_keys {
    pub const DOCK: &'static str = "dock";
    pub const INSPECTOR: &'static str = "inspector";
}

pub struct App {
    dock_state: DockState<View>,
    inspector_data: Option<ViewsData>,
    state: State,
}

impl App {
    #[cfg(not(feature = "persistence"))]
    pub fn new() -> Self {
        Self {
            dock_state: egui_dock::DockState::new(vec![View::Graph, View::GlobalTracker]),
            inspector_data: None,
            state: State::new_connected(
                RemoteInfo::default(),
                Vec::new(),
                vec![("media.category".to_owned(), "Manager".to_owned())],
                None,
            ),
        }
    }

    #[cfg(feature = "persistence")]
    pub fn new(storage: Option<&dyn eframe::Storage>) -> Self {
        let inspector_data =
            storage.and_then(|storage| eframe::get_value(storage, storage_keys::INSPECTOR));

        Self {
            dock_state: storage
                .and_then(|storage| eframe::get_value(storage, storage_keys::DOCK))
                .unwrap_or_else(|| DockState::new(vec![View::Graph, View::GlobalTracker])),

            state: State::new_connected(
                RemoteInfo::default(),
                Vec::new(),
                vec![("media.category".to_owned(), "Manager".to_owned())],
                inspector_data.as_ref(),
            ),

            inspector_data,
        }
    }

    fn disconnect(&mut self) {
        self.state.save_inspector_data(&mut self.inspector_data);
        self.state.disconnect();
    }
}

impl eframe::App for App {
    #[cfg(feature = "persistence")]
    fn auto_save_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(60 * 2)
    }

    #[cfg(feature = "persistence")]
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, storage_keys::DOCK, &self.dock_state);

        self.state.save_inspector_data(&mut self.inspector_data);

        if let Some(inspector_data) = &self.inspector_data {
            eframe::set_value(storage, storage_keys::INSPECTOR, inspector_data);
        }
    }

    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        self.state.disconnect();
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // egui won't update until there is interaction so data shown may be out of date
        ctx.request_repaint_after(std::time::Duration::from_millis(500));

        match &mut self.state {
            State::Connected { inspector, about } => {
                if inspector.process_events_or_stop() {
                    self.disconnect();
                    return;
                }

                let mut disconnect = false;
                egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
                    egui::menu::bar(ui, |ui| {
                        ui.menu_button("File", |ui| {
                            disconnect = ui
                                .button("🔌 Disconnect")
                                .on_hover_text("Disconnect from the PipeWire remote")
                                .clicked();

                            ui.separator();

                            if ui.button("❌ Quit").clicked() {
                                frame.close();
                            }
                        });

                        inspector.views_menu_buttons(ui, &mut self.dock_state);
                        inspector.tools_menu_buttons(ui);

                        ui.menu_button("Help", |ui| {
                            if ui.button("❓ About").clicked() {
                                *about = true;
                            }
                        })
                    });
                });

                if disconnect {
                    self.disconnect();
                    return;
                }

                egui::Window::new("About")
                    .collapsible(false)
                    .fixed_size([350f32, 150f32])
                    .default_pos([
                        (frame.info().window_info.size.x - 350f32) / 2f32,
                        (frame.info().window_info.size.y - 150f32) / 2f32,
                    ])
                    .open(about)
                    .show(ctx, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.heading(env!("CARGO_PKG_NAME"));
                            ui.label(env!("CARGO_PKG_VERSION"));
                            ui.label(env!("CARGO_PKG_DESCRIPTION"));

                            ui.separator();

                            ui.label("2023 Dimitris Papaioannou");
                            ui.hyperlink(env!("CARGO_PKG_REPOSITORY"));

                            ui.separator();

                            ui.label("This program is free software: you can redistribute it and/or modify it \
                                            under the terms of the GNU General Public License version 3 as published \
                                            by the Free Software Foundation.");
                        });
                    });

                inspector.tool_windows(ctx);

                let mut style = egui_dock::Style::from_egui(ctx.style().as_ref());
                style.tab.tab_body.inner_margin = egui::Margin::symmetric(5., 5.);
                egui_dock::DockArea::new(&mut self.dock_state)
                    .style(style)
                    .show_window_close_buttons(false) // Close buttons on windows do not call TabViewer::on_close
                    .show(ctx, inspector);
            }
            State::Unconnected {
                remote,
                mainloop_properties,
                context_properties,
            } => {
                let mut connect = false;
                egui::CentralPanel::default().show(ctx, |_| {});
                egui::Window::new("Connect to PipeWire")
                    .fixed_size([300., 200.])
                    .default_pos([
                        (frame.info().window_info.size.x - 300.) / 2.,
                        (frame.info().window_info.size.y - 200.) / 2.,
                    ])
                    .collapsible(false)
                    .show(ctx, |ui| {
                        ui.with_layout(egui::Layout::default().with_cross_justify(true), |ui| {
                            #[cfg(feature = "xdg_desktop_portals")]
                            egui::ComboBox::new("remote_type", "Remote kind")
                                .selected_text({
                                    match remote {
                                        RemoteInfo::Regular(..) => "Regular",
                                        RemoteInfo::Screencast { .. } => "Screencast portal",
                                        RemoteInfo::Camera => "Camera portal",
                                    }
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(remote, RemoteInfo::default(), "Regular");
                                    ui.selectable_value(
                                        remote,
                                        RemoteInfo::Screencast {
                                            types: BitFlags::EMPTY,
                                            multiple: false,
                                        },
                                        "Screencast portal",
                                    );
                                    ui.selectable_value(
                                        remote,
                                        RemoteInfo::Camera,
                                        "Camera portal",
                                    );
                                });

                            match remote {
                                RemoteInfo::Regular(name) => {
                                    egui::TextEdit::singleline(name)
                                        .hint_text("Remote name")
                                        .show(ui);
                                }

                                #[cfg(feature = "xdg_desktop_portals")]
                                RemoteInfo::Screencast { types, multiple } => {
                                    ui.horizontal(|ui| {
                                        ui.label("Source types");
                                        for (label, source_type) in [
                                            ("Monitor", SourceType::Monitor),
                                            ("Window", SourceType::Window),
                                            ("Virtual", SourceType::Virtual),
                                        ] {
                                            if ui
                                                .selectable_label(
                                                    types.contains(source_type),
                                                    label,
                                                )
                                                .clicked()
                                            {
                                                types.toggle(source_type);
                                            }
                                        }
                                    });
                                    ui.checkbox(multiple, "Multiple sources");
                                }
                                #[cfg(feature = "xdg_desktop_portals")]
                                RemoteInfo::Camera => {}
                            }
                        });

                        ui.separator();

                        for (heading, properties) in [
                            ("Mainloop properties", mainloop_properties),
                            ("Context properties", context_properties),
                        ] {
                            egui::CollapsingHeader::new(heading)
                                .show_unindented(ui, |ui| properties.show(ui));
                        }

                        ui.separator();

                        ui.with_layout(
                            egui::Layout::top_down_justified(egui::Align::Center),
                            |ui| {
                                connect = ui.button("Connect").clicked();
                            },
                        );
                    });

                if connect {
                    self.state.connect(self.inspector_data.as_ref());
                }
            }
        }
    }
}
