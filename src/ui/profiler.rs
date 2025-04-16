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

use std::{
    cell::RefCell,
    collections::{hash_map::Entry, HashMap, VecDeque},
    rc::{Rc, Weak},
};

use eframe::egui;
use egui_plot::{self, Plot, PlotPoints};

use crate::{
    backend::{
        self,
        pods::profiler::{Clock, Info, NodeBlock, Profiling},
    },
    ui::{globals_store::Global, util::uis::global_info_button},
};

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
mod data {
    use std::{
        cell::RefCell,
        collections::{btree_map::Entry, BTreeMap, VecDeque},
        rc::Weak,
    };

    use egui_plot::PlotPoints;

    use crate::{
        backend::pods::profiler::{NodeBlock, Profiling},
        ui::globals_store::Global,
    };

    fn pop_front_push_back<T>(queue: &mut VecDeque<T>, max: usize, value: T) {
        if queue.len() + 1 > max {
            queue.pop_front();
        }

        queue.push_back(value);
    }

    fn generate_plot_points(points: impl Iterator<Item = f64>) -> PlotPoints<'static> {
        PlotPoints::from_iter(points.enumerate().map(|(i, x)| [i as f64, x]))
    }

    struct ClientMeasurement {
        end_date: f64,
        scheduling_latency: f64,
        duration: f64,
    }

    impl ClientMeasurement {
        const fn empty() -> Self {
            Self {
                end_date: f64::NAN,
                scheduling_latency: f64::NAN,
                duration: f64::NAN,
            }
        }

        fn new(follower: &NodeBlock, driver: &NodeBlock) -> Self {
            Self {
                end_date: (follower.finish - driver.signal) as f64 / 1000.,
                scheduling_latency: (follower.awake - follower.signal) as f64 / 1000.,
                duration: (follower.finish - follower.awake) as f64 / 1000.,
            }
        }
    }

    pub struct Client {
        last_profiling: Option<NodeBlock>,

        title: String,
        measurements: VecDeque<ClientMeasurement>,

        // Position of last non-empty profiling that was added.
        // When this reaches 0 every profiling is empty indicating
        // that this follower has no statistics to show
        last_non_empty_pos: usize,

        // Stored weakly as these objects live for as long as there
        // are stored profilings of them, which can be longer than
        // the lifetime of the global
        pub global: Weak<RefCell<Global>>,
    }

    impl Client {
        fn new(title: String, max_profilings: usize, global: Weak<RefCell<Global>>) -> Self {
            Self {
                last_profiling: None,

                title,
                measurements: VecDeque::with_capacity(max_profilings),

                last_non_empty_pos: max_profilings,

                global,
            }
        }

        pub fn title(&self) -> &str {
            &self.title
        }

        fn add_measurement(
            &mut self,
            follower: &NodeBlock,
            driver: &NodeBlock,
            max_profilings: usize,
        ) {
            pop_front_push_back(
                &mut self.measurements,
                max_profilings,
                ClientMeasurement::new(follower, driver),
            );

            self.last_profiling = Some(follower.clone());
            self.last_non_empty_pos = self.measurements.len();
        }

        fn add_empty_measurement(&mut self, max_profilings: usize) {
            pop_front_push_back(
                &mut self.measurements,
                max_profilings,
                ClientMeasurement::empty(),
            );

            self.last_non_empty_pos -= 1;

            self.last_profiling = None;
        }

        const fn is_empty(&self) -> bool {
            self.last_non_empty_pos == 0
        }

        pub const fn last_profiling(&self) -> Option<&NodeBlock> {
            self.last_profiling.as_ref()
        }

        pub fn end_date(&self) -> PlotPoints {
            generate_plot_points(self.measurements.iter().map(|m| m.end_date))
        }
        pub fn scheduling_latency(&self) -> PlotPoints {
            generate_plot_points(self.measurements.iter().map(|m| m.scheduling_latency))
        }
        pub fn duration(&self) -> PlotPoints {
            generate_plot_points(self.measurements.iter().map(|m| m.duration))
        }
    }

    struct DriverMeasurement {
        delay: f64,
        period: f64,
        estimated: f64,
        end_date: f64,
    }

    impl From<&Profiling> for DriverMeasurement {
        fn from(p: &Profiling) -> Self {
            Self {
                delay: (p.clock.delay * 1_000_000) as f64 / f64::from(p.clock.rate.denom),

                period: ((p.driver.signal - p.driver.prev_signal) / 1000) as f64,

                end_date: ((p.driver.finish - p.driver.signal) / 1000) as f64,

                estimated: (p.clock.duration * 1_000_000) as f64
                    / (p.clock.rate_diff * f64::from(p.clock.rate.denom)),
            }
        }
    }

    pub struct Driver {
        last_profiling: Option<Profiling>,

        measurements: VecDeque<DriverMeasurement>,
        followers: BTreeMap<i32, Client>,

        // Stored weakly as these objects live for as long as there
        // are stored profilings of them, which can be longer than
        // the lifetime of the global
        pub global: Weak<RefCell<Global>>,
    }

    impl Driver {
        pub fn with_max_profilings(max_profilings: usize, global: Weak<RefCell<Global>>) -> Self {
            Self {
                last_profiling: None,

                measurements: VecDeque::with_capacity(max_profilings),
                followers: BTreeMap::new(),

                global,
            }
        }

        pub fn add_profiling(
            &mut self,
            profiling: Profiling,
            max_profilings: usize,
            global_getter: &impl Fn(i32) -> Option<Weak<RefCell<Global>>>,
        ) {
            pop_front_push_back(
                &mut self.measurements,
                max_profilings,
                DriverMeasurement::from(&profiling),
            );

            // Add measurements to registered followers and delete those that have no non-empty measurements
            self.followers.retain(|id, follower| {
                if let Some(f) = profiling.followers.iter().find(|nb| nb.id == *id) {
                    follower.add_measurement(f, &profiling.driver, max_profilings);
                } else {
                    follower.add_empty_measurement(max_profilings);
                }

                !follower.is_empty()
            });

            // Add new followers or update their referenced globals (PipeWire reuses IDs for globals)
            for follower in &profiling.followers {
                match self.followers.entry(follower.id) {
                    Entry::Occupied(mut e) => {
                        let client = e.get_mut();

                        if client.global.upgrade().is_none() {
                            if let Some(global) = global_getter(follower.id) {
                                client.global = global;
                            }
                        }
                    }
                    Entry::Vacant(e) => {
                        if let Some(global) = global_getter(follower.id) {
                            e.insert(Client::new(
                                format!("{}/{}", follower.name, follower.id),
                                max_profilings,
                                global,
                            ))
                            .add_measurement(
                                follower,
                                &profiling.driver,
                                max_profilings,
                            );
                        }
                    }
                }
            }

            self.last_profiling = Some(profiling);
        }

        pub const fn last_profling(&self) -> Option<&Profiling> {
            self.last_profiling.as_ref()
        }

        pub fn name(&self) -> Option<&str> {
            self.last_profling().map(|p| p.driver.name.as_str())
        }

        pub fn clear(&mut self) {
            self.measurements.clear();
            self.followers.clear();
        }

        pub fn adjust_queues(&mut self, max_profilings: usize) {
            fn adjust_queue<T>(queue: &mut VecDeque<T>, max: usize) {
                if queue.capacity() < max {
                    queue.reserve(max - queue.len());
                } else if queue.len() > max {
                    queue.drain(0..(queue.len() - max));
                }
            }

            adjust_queue(&mut self.measurements, max_profilings);
            for follower in self.followers.values_mut() {
                adjust_queue(&mut follower.measurements, max_profilings);
            }
        }

        pub fn delay(&self) -> PlotPoints {
            generate_plot_points(self.measurements.iter().map(|m| m.delay))
        }

        pub fn period(&self) -> PlotPoints {
            generate_plot_points(self.measurements.iter().map(|m| m.period))
        }

        pub fn estimated(&self) -> PlotPoints {
            generate_plot_points(self.measurements.iter().map(|m| m.estimated))
        }

        pub fn end_date(&self) -> PlotPoints {
            generate_plot_points(self.measurements.iter().map(|m| m.end_date))
        }

        pub fn clients(&self) -> impl Iterator<Item = &Client> {
            self.followers.values()
        }

        pub fn n_clients(&self) -> usize {
            self.followers.len()
        }
    }
}

use data::{Client, Driver};

pub struct Profiler {
    max_profilings: usize,
    drivers: HashMap<i32, Driver>,
    selected_driver_id: Option<i32>,
    pause: bool,

    /// Temporarily holds incoming data until the update interval passes
    buffer: VecDeque<Profiling>,

    // Used for updating last profilings of nodes periodically instead of on every new profiling.
    // This is useful for not drawing new data on every egui update, such as mouse movement
    last_profs_update: std::time::Instant,
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
impl Profiler {
    pub fn with_max_profilings(max_profilings: usize) -> Self {
        Self {
            max_profilings,
            drivers: HashMap::new(),
            selected_driver_id: None,
            pause: false,

            buffer: VecDeque::new(),

            last_profs_update: std::time::Instant::now(),
        }
    }

    fn update_data(
        &mut self,
        update_rate: std::time::Duration,
        global_getter: impl Fn(i32) -> Option<Weak<RefCell<Global>>>,
    ) {
        // No need to query the clock, refresh instantly
        if !update_rate.is_zero() {
            let now = std::time::Instant::now();

            if now.duration_since(self.last_profs_update) >= update_rate {
                self.last_profs_update = now;
            } else {
                return;
            }
        }

        for driver in self.drivers.values_mut() {
            driver.adjust_queues(self.max_profilings);
        }

        for p in self.buffer.drain(..) {
            match self.drivers.entry(p.driver.id) {
                Entry::Occupied(mut e) => {
                    e.get_mut()
                        .add_profiling(p, self.max_profilings, &global_getter);
                }
                Entry::Vacant(e) => {
                    if let Some(global) = global_getter(p.driver.id) {
                        e.insert(Driver::with_max_profilings(self.max_profilings, global))
                            .add_profiling(p, self.max_profilings, &global_getter);
                    }
                }
            }
        }
    }

    pub fn add_profilings(&mut self, profilings: Vec<Profiling>) {
        if self.pause {
            return;
        }

        let skip = if profilings.len() >= self.max_profilings {
            self.buffer.clear();
            profilings.len() - self.max_profilings
        } else if profilings.len() + self.buffer.len() > self.max_profilings {
            self.buffer
                .drain(0..usize::min(self.buffer.len(), profilings.len()));
            0
        } else {
            0
        };

        self.buffer.extend(profilings.into_iter().skip(skip));
    }

    pub fn show_profiler(
        &mut self,
        ui: &mut egui::Ui,
        sx: &backend::Sender,
        update_rate: std::time::Duration,
        global_getter: impl Fn(i32) -> Option<Weak<RefCell<Global>>>,
    ) {
        if ui
            .small_button("Reset")
            .on_hover_text("Clear all profiling data")
            .clicked()
        {
            self.drivers.clear();
            self.buffer.clear();
            self.max_profilings = 250;
            self.selected_driver_id = None;
            self.pause = false;
            return;
        }

        self.update_data(update_rate, global_getter);

        let Some((id, driver)) = ({
            let driver = self
                .selected_driver_id
                .and_then(|id| self.drivers.get(&id).map(|d| (id, d)));

            // Selected driver doesn't exist
            if self.selected_driver_id.is_some() && driver.is_none() {
                self.selected_driver_id = None;
            }

            let cb = egui::ComboBox::from_label("Driver");
            if let Some(name) = driver.as_ref().map(|(_, d)| d.name()) {
                cb.selected_text(name.unwrap_or("Unnamed driver"))
            } else {
                cb.selected_text("Select a driver")
            }
            .show_ui(ui, |ui| {
                for (id, driver) in &self.drivers {
                    let name = driver
                        .name()
                        .map_or_else(|| format!("Unnamed driver {id}"), ToOwned::to_owned);

                    ui.selectable_value(&mut self.selected_driver_id, Some(*id), name);
                }
            });

            driver
        }) else {
            ui.label("Select a driver to view profiling info");
            return;
        };

        ui.horizontal(|ui| {
            global_info_button(ui, driver.global.upgrade().as_ref(), sx);
            ui.label(format!("Driver ID: {id}"));
        });

        egui::CollapsingHeader::new("Last profiling info").default_open(true).show(ui, |ui| {
            if let Some(last) = driver.last_profling() {
                let info = &last.info;
                let followers = last.followers.len();
                ui.label(format!(
                    "Total profiler samples: {} | Xruns: {} | Follower nodes: {}\nQuantum: {} | CPU Load: {} {} {}",
                    info.counter, info.xrun_count, followers, last.clock.duration * i64::from(last.clock.rate.num), info.cpu_load_fast, info.cpu_load_medium, info.cpu_load_slow));
            }
        });

        let clear = ui.horizontal(|ui| {
            ui.label("Profilings");
            ui.add(egui::widgets::DragValue::new(&mut self.max_profilings).range(1..=1_000_000))
                .on_hover_text("Number of profiler samples to keep in memory. Very big values will slow down the application.");

            let clear = ui.button("Clear driver samples").clicked();

            ui.toggle_value(&mut self.pause, "Pause");

            clear
        }).inner;
        if clear {
            self.drivers.get_mut(&id).unwrap().clear();
            return;
        }

        if ui.input(|i| i.focused && i.key_pressed(egui::Key::Space)) {
            self.pause = !self.pause;
        }

        fn profiler_plot(
            ui: &mut egui::Ui,
            heading: &str,
            explanation: &str,
            id: &str,
            max_x: usize,
        ) -> Plot<'static> {
            let reset = ui
                .horizontal(|ui| {
                    ui.heading(heading).on_hover_text(explanation);
                    ui.small_button("Reset").clicked()
                })
                .inner;

            let plot = Plot::new(id)
                .clamp_grid(true)
                .legend(egui_plot::Legend::default())
                .allow_zoom(egui::emath::Vec2b::new(true, false))
                .allow_drag(egui::emath::Vec2b::new(true, false))
                .label_formatter(|name, value| {
                    if name.is_empty() {
                        String::new()
                    } else {
                        format!("{name}: {:.0}us\nProcess cycle: {:.0}", value.y, value.x)
                    }
                })
                .x_axis_formatter(move |x, _| {
                    let x = x.value;

                    if x.is_sign_negative() || x > max_x as f64 || x % 1. != 0. {
                        String::new()
                    } else {
                        format!("{x:.0}")
                    }
                })
                .y_axis_formatter(|y, _| {
                    let y = y.value;
                    if y.is_sign_negative() {
                        String::new()
                    } else {
                        format!("{y}us")
                    }
                });

            if reset {
                plot.reset()
            } else {
                plot
            }
        }

        ui.separator();

        ui.columns_const::<2, _>( |ui| {
            profiler_plot(
                &mut ui[0],
                "Driver Timing",
                "Delay: Delay to device\n\
                              Period: Time between when the previous cycle started and when the current cycle started\n\
                              Estimated: Estimated time until the next cycle starts",
                "driver_timing",
                self.max_profilings,
            )
            .height(ui[0].available_height() / 2.)
            .show(&mut ui[0], |ui| {
                for (name, plot_points) in [
                    ("Driver Delay", driver.delay()),
                    ("Period", driver.period()),
                    ("Estimated", driver.estimated()),
                ] {
                    ui.line(egui_plot::Line::new(plot_points).name(name));
                }
            });

            profiler_plot(
                &mut ui[1],
                "Driver End Date",
                "Time between when the current cycle started and when the driver finished processing/current cycle ended",
                "driver_end_date",
                self.max_profilings,
            )
            .height(ui[1].available_height() / 2.)
            .show(&mut ui[1], |ui| {
                ui.line(egui_plot::Line::new(driver.end_date()).name("Driver End Date"));
            });
        });

        ui.separator();

        ui.columns_const::<3, _>(|ui| {
            for (i, (heading, explanation, id, measurement)) in [
                (
                    "Clients End Date",
                    "Time between when the current cycle started and when the client finished processing",
                    "clients_end_date",
                    Client::end_date as fn(&Client) -> PlotPoints,
                ),
                (
                    "Clients Scheduling Latency",
                    "Time between when the client was ready to start processing and when it actually started processing",
                    "clients_scheduling_latency",
                    Client::scheduling_latency,
                ),
                ("Clients Duration", "Time between when the client started processing and when it finished and woke up the next nodes in the graph", "clients_duration", Client::duration),
            ]
            .into_iter()
            .enumerate()
            {
                profiler_plot(&mut ui[i], heading, explanation, id, self.max_profilings).show(
                    &mut ui[i],
                    |ui| {
                        for client in driver.clients() {
                            ui.line(egui_plot::Line::new(measurement(client)).name(client.title()));
                        }
                    },
                );
            }
        });
    }

    pub fn show_process_viewer(
        &mut self,
        ui: &mut egui::Ui,
        sx: &backend::Sender,
        update_rate: std::time::Duration,
        global_getter: impl Fn(i32) -> Option<Weak<RefCell<Global>>>,
    ) {
        if ui
            .small_button("Reset")
            .on_hover_text("Clear all profiling data")
            .clicked()
        {
            self.drivers.clear();
            self.selected_driver_id = None;
            self.pause = false;
            return;
        }

        self.update_data(update_rate, global_getter);

        ui.separator();

        fn draw_chart(driver: &Driver, ui: &mut egui::Ui) {
            use egui_plot::{Bar, BarChart};

            let mut wait = Vec::with_capacity(driver.n_clients());
            let mut busy = Vec::with_capacity(driver.n_clients());
            let mut y_labels = Vec::with_capacity(driver.n_clients());

            for (i, nb) in driver
                .clients()
                .map(|f| f.last_profiling())
                .chain(std::iter::once(driver.last_profling().map(|lp| &lp.driver))) // NodeBlock of the driver
                .flatten()
                .enumerate()
            {
                wait.push(Bar::new(i as f64, (nb.awake - nb.signal) as f64 / 1000.).horizontal());
                busy.push(Bar::new(i as f64, (nb.finish - nb.awake) as f64 / 1000.).horizontal());
                y_labels.push(nb.name.as_str());
            }

            ui.set_width(ui.available_width());

            Plot::new("Chart")
                .height((y_labels.len() * 45) as f32)
                .allow_drag(false)
                .allow_zoom(false)
                .allow_scroll(false)
                .clamp_grid(true)
                .show_grid(egui::Vec2b::new(true, false))
                .set_margin_fraction(egui::vec2(0.01, 0.35))
                .x_axis_formatter(|grid_mark, _| format!("{} us", grid_mark.value))
                .y_axis_formatter(|grid_mark, _| {
                    if grid_mark.value.is_sign_positive()
                        && (grid_mark.value as usize) < y_labels.len()
                        && grid_mark.value % 1. == 0.
                    {
                        y_labels[grid_mark.value as usize].to_owned()
                    } else {
                        String::new()
                    }
                })
                .label_formatter(|_, _| String::new())
                .legend(
                    egui_plot::Legend::default()
                        .position(egui_plot::Corner::LeftTop)
                        .text_style(egui::TextStyle::Small),
                )
                .show(ui, |plot_ui| {
                    let wait = BarChart::new(wait)
                        .name("Waiting")
                        .element_formatter(Box::new(|b, _| format!("Waiting took {} us", b.value)));

                    let busy = BarChart::new(busy)
                        .name("Busy")
                        .stack_on(&[&wait])
                        .element_formatter(Box::new(|b, _| {
                            format!("Processing took {} us", b.value)
                        }));

                    plot_ui.bar_chart(wait);
                    plot_ui.bar_chart(busy);
                });
        }

        fn draw_node_block(
            block: &NodeBlock,
            clock: &Clock,
            info: &Info,
            driver: bool,
            global: Option<&Rc<RefCell<Global>>>,
            ui: &mut egui::Ui,
            sx: &backend::Sender,
        ) {
            global_info_button(ui, global, sx);

            ui.label(block.id.to_string());
            ui.label(&block.name);

            // Quantum, Rate
            if driver {
                ui.label((clock.duration * i64::from(clock.rate.num)).to_string());
                ui.label(clock.rate.denom.to_string());
            } else {
                for n in [block.latency.num, block.latency.denom] {
                    if n == 0 {
                        ui.label("Using driver's");
                    } else {
                        ui.label(n.to_string());
                    }
                }
            }

            fn format_to_time(nanos: i64) -> String {
                let nanos = nanos as f64;
                if nanos < 1_000_000. {
                    format!("{:.3}us", nanos / 1000.)
                } else if nanos < 1_000_000_000. {
                    format!("{:.4}ms", nanos / 1_000_000.)
                } else {
                    format!("{:.6}s", nanos / 1_000_000_000.)
                }
            }

            // Waiting
            if block.awake >= block.signal {
                ui.label(format_to_time(block.awake - block.signal));
            } else if block.signal > block.prev_signal {
                ui.label("Did not wake");
            } else {
                ui.label("Was not signaled");
            };

            // Busy
            if block.finish >= block.awake {
                ui.label(format_to_time(block.finish - block.awake));
            } else if block.awake > block.prev_signal {
                ui.label("Did not complete");
            } else {
                ui.label("Did not start");
            }

            // Waiting/Quantum, Busy/Quantum
            let quantum =
                clock.duration as f64 * f64::from(clock.rate.num) / f64::from(clock.rate.denom);
            for n in [block.awake - block.signal, block.finish - block.awake] {
                ui.label(format!("{:.6}", n as f64 / 1_000_000_000. / quantum));
            }

            // Xruns
            if let Some(xruns) = block.xrun_count {
                ui.label(xruns.to_string());
            } else {
                ui.label(info.xrun_count.to_string());
            }
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            self.drivers.retain(|id, driver| {
                if let Some(p) = driver.last_profling() {
                    let keep = ui.horizontal(|ui| {
                        let keep = !ui.small_button("Delete").clicked();
                        if let Some(p) = driver.last_profling() {
                            ui.label(format!("Driver: {} (ID: {id})", &p.driver.name));
                        } else {
                            ui.label(format!("Driver ID: {id}"));
                        }
                        keep
                    }).inner;

                    ui.push_id(id, |ui| {
                        egui::ScrollArea::horizontal().show(ui, |ui| {
                            egui::Grid::new("timings")
                            .striped(true)
                            .num_columns(10)
                            .min_col_width(0.0)
                            .show(ui, |ui| {
                                ui.label("");
                                ui.label("ID");
                                ui.label("Name");
                                ui.label("Quantum");
                                ui.label("Rate");
                                ui.label("Waiting").on_hover_text("Time between when the node was ready to start processing and when it actually started processing");
                                ui.label("Busy").on_hover_text("Time between when the node started processing and when it finished and woke up the next nodes in the graph");
                                ui.label("Waiting/Quantum").on_hover_text("A measure of the graph load");
                                ui.label("Busy/Quantum").on_hover_text("A measure of the load of the driver/node");
                                ui.label("Xruns");
                                ui.end_row();

                                draw_node_block(&p.driver, &p.clock, &p.info, true, driver.global.upgrade().as_ref(), ui, sx);
                                ui.end_row();

                                for (client, nb) in driver.clients().filter_map(|c| c.last_profiling().map(|p| (c.global.upgrade(), p))) {
                                    draw_node_block(nb, &p.clock, &p.info, false, client.as_ref(), ui, sx);
                                    ui.end_row();
                                }
                            });
                        });
                    });

                    egui::CollapsingHeader::new("Chart").id_salt(id).show(ui, |ui| {
                        draw_chart(driver, ui);
                    });

                    ui.separator();

                    keep
                } else {
                    true
                }
            });
        });
    }
}
