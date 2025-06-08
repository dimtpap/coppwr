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

use std::time::Duration;

use eframe::egui;
use egui_dock::DockState;

#[cfg(feature = "xdg_desktop_portals")]
use ashpd::{desktop::screencast::SourceType, enumflags2::BitFlags};

use crate::{backend::RemoteInfo, ui::util::uis::EditableKVList};

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

#[cfg_attr(
    feature = "persistence",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
struct Settings {
    update_rate: Duration,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            update_rate: Duration::from_millis(500),
        }
    }
}

mod inspector {
    use std::rc::Rc;

    use eframe::egui;

    use pipewire::types::ObjectType;

    use crate::{
        backend::{self, Event, RemoteInfo},
        ui::{
            globals_store::ObjectData,
            graph::MediaType,
            util::{persistence::PersistentView, tool::Windowed},
            ContextManager, GlobalsStore, Graph, MetadataEditor, ObjectCreator, Profiler,
        },
    };

    use super::{Settings, View};

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
            &mut self,
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
                        ObjectType::Metadata => self.metadata_editor.tool.add_metadata(global),

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

                                    let media_type =
                                        global_borrow.props().get("format.dsp").map(|format_dsp| {
                                            if format_dsp.ends_with("audio") {
                                                MediaType::Audio
                                            } else if format_dsp.ends_with("midi")
                                                || format_dsp.ends_with("UMP")
                                            {
                                                MediaType::Midi
                                            } else if format_dsp.ends_with("video") {
                                                MediaType::Video
                                            } else {
                                                MediaType::Unknown
                                            }
                                        });

                                    match info[0].1.as_str() {
                                        "Input" => {
                                            self.graph.add_input_port(id, parent, name, media_type);
                                        }
                                        "Output" => {
                                            self.graph
                                                .add_output_port(id, parent, name, media_type);
                                        }
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
                    self.graph.show(ui, &mut self.handle.sx);
                }
            }
        }
    }
}

use inspector::{Inspector, PersistentData};

/// Represents the PipeWire connection state.
enum State {
    Connected(Inspector),
    Unconnected {
        remote: RemoteInfo,
        mainloop_properties: EditableKVList,
        context_properties: EditableKVList,
    },
}

impl Default for State {
    fn default() -> Self {
        let mut context_properties = EditableKVList::new();
        context_properties
            .list
            .push(("media.category".to_owned(), "Manager".to_owned()));

        Self::Unconnected {
            remote: RemoteInfo::default(),
            mainloop_properties: EditableKVList::new(),
            context_properties,
        }
    }
}

impl State {
    fn new_connected(
        remote: RemoteInfo,
        mainloop_properties: Vec<(String, String)>,
        context_properties: Vec<(String, String)>,
        inspector_data: Option<&PersistentData>,
    ) -> Self {
        Self::Connected(Inspector::new(
            remote,
            mainloop_properties,
            context_properties,
            inspector_data,
        ))
    }

    fn save_inspector_data(&self, data: &mut Option<PersistentData>) {
        if let Self::Connected(inspector) = self {
            inspector.save_data(data);
        }
    }
}

#[cfg(feature = "persistence")]
mod storage_keys {
    pub const DOCK: &str = "dock";
    pub const INSPECTOR: &str = "inspector";
    pub const SETTINGS: &str = "settings";
}

pub struct App {
    dock_state: DockState<View>,
    inspector_data: Option<PersistentData>,
    settings: Settings,
    about_open: bool,
    state: State,

    #[cfg(feature = "xdg_desktop_portals")]
    _system_theme_listener: crate::system_theme_listener::SystemThemeListener,
}

impl App {
    #[cfg(not(feature = "persistence"))]
    pub fn new(_cc: &eframe::CreationContext) -> Self {
        Self {
            dock_state: egui_dock::DockState::new(vec![View::Graph, View::GlobalTracker]),
            inspector_data: None,
            settings: Settings::default(),
            about_open: false,
            state: State::new_connected(
                RemoteInfo::default(),
                Vec::new(),
                vec![("media.category".to_owned(), "Manager".to_owned())],
                None,
            ),

            #[cfg(feature = "xdg_desktop_portals")]
            _system_theme_listener: crate::system_theme_listener::SystemThemeListener::new(
                &_cc.egui_ctx,
            ),
        }
    }

    #[cfg(feature = "persistence")]
    pub fn new(cc: &eframe::CreationContext) -> Self {
        let (dock_state, settings, inspector_data) =
            cc.storage.map_or((None, None, None), |storage| {
                (
                    eframe::get_value(storage, storage_keys::DOCK),
                    eframe::get_value(storage, storage_keys::SETTINGS),
                    eframe::get_value(storage, storage_keys::INSPECTOR),
                )
            });

        Self {
            dock_state: dock_state
                .unwrap_or_else(|| DockState::new(vec![View::Graph, View::GlobalTracker])),

            state: State::new_connected(
                RemoteInfo::default(),
                Vec::new(),
                vec![("media.category".to_owned(), "Manager".to_owned())],
                inspector_data.as_ref(),
            ),

            settings: settings.unwrap_or_default(),

            about_open: false,

            inspector_data,

            #[cfg(feature = "xdg_desktop_portals")]
            _system_theme_listener: crate::system_theme_listener::SystemThemeListener::new(
                &cc.egui_ctx,
            ),
        }
    }

    fn disconnect(&mut self) {
        self.state.save_inspector_data(&mut self.inspector_data);
        self.state = State::default();
    }

    fn about_ui(ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.heading(env!("CARGO_PKG_NAME"));
            ui.label(env!("CARGO_PKG_VERSION"));
            ui.label(env!("CARGO_PKG_DESCRIPTION"));

            ui.separator();

            ui.label("2023-2025 Dimitris Papaioannou");
            ui.hyperlink(env!("CARGO_PKG_REPOSITORY"));

            ui.separator();

            ui.label("This program is free software: you can redistribute it and/or modify it \
                            under the terms of the GNU General Public License version 3 as published \
                            by the Free Software Foundation.");
        });
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

        eframe::set_value(storage, storage_keys::SETTINGS, &self.settings);
    }

    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        // Switch to stop backend
        // Not using Default to avoid allocations
        self.state = State::Unconnected {
            remote: RemoteInfo::Regular(String::new()),
            mainloop_properties: EditableKVList::new(),
            context_properties: EditableKVList::new(),
        };
    }

    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        // egui won't update until there is interaction so data shown may be out of date
        ctx.request_repaint_after(
            self.settings.update_rate + Duration::from_millis(20),
            // https://github.com/emilk/egui/commit/0be4450e3da62918339a4cb3113da4a31d033b52#diff-dc1fee2debc9928daf5514f8678c2bedafc1d679871fb75a7b0ff5450ecf7431R168
            // causes updates to be missed by UIs that update strictly after `update_rate`
            // since the first update comes too early
        );

        let window_size = ctx.input(|i| i.screen_rect()).size();

        match &mut self.state {
            State::Connected(inspector) => {
                struct Viewer<'a, 'b>(&'a mut Inspector, &'b Settings);

                impl egui_dock::TabViewer for Viewer<'_, '_> {
                    type Tab = View;

                    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
                        self.0.show_view(ui, *tab, self.1);
                    }

                    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
                        tab.as_str().into()
                    }

                    fn scroll_bars(&self, _tab: &Self::Tab) -> [bool; 2] {
                        [false, false]
                    }
                }

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
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        });

                        inspector.views_menu_buttons(ui, &mut self.dock_state);
                        inspector.tools_menu_buttons(ui);

                        ui.menu_button("Settings", |ui| {
                            ui.horizontal(|ui| {
                                ui.label("🔁 Update Rate").on_hover_text(
                                    "How often to refresh the UI with new data from PipeWire. Lower values result in higher CPU usage.",
                                );
                                ui.add(
                                    egui::DragValue::from_get_set(|v| {
                                        if let Some(v) = v {
                                            self.settings.update_rate = Duration::from_secs_f64(v);
                                            v
                                        } else {
                                            self.settings.update_rate.as_secs_f64()
                                        }
                                    })
                                    .range(0f64..=86_400f64)
                                    .speed(0.001)
                                    .custom_parser(|v| v.parse::<f64>().ok().map(|v| v / 1000.))
                                    .custom_formatter(|n, _| format!("{:.0}ms", n * 1000.)),
                                );
                            });

                            ui.separator();

                            ui.label("🎨 Theme");

                            let mut theme_preference = ctx.options(|o| o.theme_preference);

                            let mut changed = false;

                            #[cfg(feature = "xdg_desktop_portals")]
                            ui.horizontal(|ui| {
                                changed = ui.radio_value(&mut theme_preference, egui::ThemePreference::System, "Use system's").changed();

                                if !self._system_theme_listener.is_running() {
                                    ui.label("⚠").on_hover_text("Cannot access the system theme.\nEither the portal is not available,\nor an error occurred. (See stderr)");
                                }
                            });

                            for (pref, text) in [
                                (egui::ThemePreference::Dark, "Dark"),
                                (egui::ThemePreference::Light, "Light")
                            ] {
                                changed |=
                                    ui.radio_value(&mut theme_preference, pref, text).changed();
                            }

                            if changed {
                                ctx.set_theme(theme_preference);
                            }
                        });

                        ui.menu_button("Help", |ui| {
                            if ui.button("❓ About").clicked() {
                                self.about_open = true;
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
                    .pivot(egui::Align2::CENTER_CENTER)
                    .default_pos([window_size.x / 2f32, window_size.y / 2f32])
                    .open(&mut self.about_open)
                    .show(ctx, Self::about_ui);

                inspector.tool_windows(ctx);

                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(ctx.style().visuals.panel_fill)) // No margins
                    .show(ctx, |ui| {
                        egui_dock::DockArea::new(&mut self.dock_state)
                            .show_inside(ui, &mut Viewer(inspector, &self.settings));
                    });
            }
            State::Unconnected {
                remote,
                mainloop_properties,
                context_properties,
            } => {
                let mut connect = false;
                egui::CentralPanel::default().show(ctx, |_| {});
                egui::Modal::new("connect_prompt".into())
                    .area(
                        egui::Modal::default_area("connect_prompt_area".into())
                            .default_size([300., 200.]),
                    )
                    .show(ctx, {
                        let mainloop_properties = &mut *mainloop_properties;
                        |ui| {
                            ui.with_layout(
                                ui.layout().clone().with_cross_align(egui::Align::Center),
                                |ui| {
                                    ui.heading("Connect to PipeWire");
                                },
                            );

                            ui.separator();

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
                                        .desired_width(f32::INFINITY)
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
                        }
                    });

                if connect {
                    self.state = State::new_connected(
                        std::mem::replace(remote, RemoteInfo::Regular(String::new())),
                        std::mem::take(&mut mainloop_properties.list),
                        std::mem::take(&mut context_properties.list),
                        self.inspector_data.as_ref(),
                    );
                }
            }
        }
    }
}
