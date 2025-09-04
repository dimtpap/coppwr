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
    collections::{HashMap, VecDeque, hash_map::Entry},
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
        collections::{BTreeMap, VecDeque, btree_map::Entry},
        rc::Weak,
    };

    use egui_plot::{PlotPoint, PlotPoints};

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

    fn adjust_queue<T>(queue: &mut VecDeque<T>, max: usize) {
        if queue.capacity() < max {
            queue.reserve(max - queue.len());
        } else if queue.len() > max {
            queue.drain(0..(queue.len() - max));
        }
    }

    fn generate_plot_points(points: impl Iterator<Item = f64>) -> PlotPoints<'static> {
        PlotPoints::Owned(
            points
                .enumerate()
                .map(|(i, x)| PlotPoint { x: i as f64, y: x })
                .collect(),
        )
    }

    struct ClientMeasurements {
        end_date: VecDeque<f64>,
        scheduling_latency: VecDeque<f64>,
        duration: VecDeque<f64>,
    }

    impl ClientMeasurements {
        fn with_max_profilings(max: usize) -> Self {
            Self {
                end_date: VecDeque::with_capacity(max),
                scheduling_latency: VecDeque::with_capacity(max),
                duration: VecDeque::with_capacity(max),
            }
        }

        fn len(&self) -> usize {
            assert!(
                self.end_date.len() == self.scheduling_latency.len()
                    && self.scheduling_latency.len() == self.duration.len()
            );

            self.end_date.len()
        }

        fn end_date(&self) -> impl Iterator<Item = f64> {
            self.end_date.iter().copied()
        }

        fn scheduling_latency(&self) -> impl Iterator<Item = f64> {
            self.scheduling_latency.iter().copied()
        }

        fn duration(&self) -> impl Iterator<Item = f64> {
            self.duration.iter().copied()
        }

        fn add_empty(&mut self, max: usize) {
            pop_front_push_back(&mut self.end_date, max, f64::NAN);
            pop_front_push_back(&mut self.scheduling_latency, max, f64::NAN);
            pop_front_push_back(&mut self.duration, max, f64::NAN);
        }

        fn push(&mut self, max: usize, follower: &NodeBlock, driver: &NodeBlock) {
            let end_date = (follower.finish - driver.signal) as f64 / 1000.;
            let scheduling_latency = (follower.awake - follower.signal) as f64 / 1000.;
            let duration = (follower.finish - follower.awake) as f64 / 1000.;

            pop_front_push_back(&mut self.end_date, max, end_date);
            pop_front_push_back(&mut self.scheduling_latency, max, scheduling_latency);
            pop_front_push_back(&mut self.duration, max, duration);
        }

        fn adjust_queues(&mut self, max: usize) {
            adjust_queue(&mut self.end_date, max);
            adjust_queue(&mut self.scheduling_latency, max);
            adjust_queue(&mut self.duration, max);
        }
    }

    pub struct Client {
        last_profiling: Option<NodeBlock>,

        title: String,
        measurements: ClientMeasurements,

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
                measurements: ClientMeasurements::with_max_profilings(max_profilings),

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
            self.measurements.push(max_profilings, follower, driver);

            self.last_profiling = Some(follower.clone());
            self.last_non_empty_pos = self.measurements.len();
        }

        fn add_empty_measurement(&mut self, max_profilings: usize) {
            self.measurements.add_empty(max_profilings);

            self.last_non_empty_pos -= 1;

            self.last_profiling = None;
        }

        const fn is_empty(&self) -> bool {
            self.last_non_empty_pos == 0
        }

        pub const fn last_profiling(&self) -> Option<&NodeBlock> {
            self.last_profiling.as_ref()
        }

        pub fn end_date(&self) -> PlotPoints<'_> {
            generate_plot_points(self.measurements.end_date())
        }
        pub fn scheduling_latency(&self) -> PlotPoints<'_> {
            generate_plot_points(self.measurements.scheduling_latency())
        }
        pub fn duration(&self) -> PlotPoints<'_> {
            generate_plot_points(self.measurements.duration())
        }
    }

    struct DriverMeasurements {
        delay: VecDeque<f64>,
        period: VecDeque<f64>,
        estimated: VecDeque<f64>,
        end_date: VecDeque<f64>,
    }

    impl DriverMeasurements {
        fn with_max_profilings(max: usize) -> Self {
            Self {
                delay: VecDeque::with_capacity(max),
                period: VecDeque::with_capacity(max),
                estimated: VecDeque::with_capacity(max),
                end_date: VecDeque::with_capacity(max),
            }
        }

        fn delay(&self) -> impl Iterator<Item = f64> {
            self.delay.iter().copied()
        }

        fn period(&self) -> impl Iterator<Item = f64> {
            self.period.iter().copied()
        }

        fn estimated(&self) -> impl Iterator<Item = f64> {
            self.estimated.iter().copied()
        }

        fn end_date(&self) -> impl Iterator<Item = f64> {
            self.end_date.iter().copied()
        }

        fn push(&mut self, max: usize, p: &Profiling) {
            let delay = (p.clock.delay * 1_000_000) as f64 / f64::from(p.clock.rate.denom);

            let period = (p.driver.signal - p.driver.prev_signal) as f64 / 1000.;

            let estimated = (p.clock.duration * 1_000_000) as f64
                / (p.clock.rate_diff * f64::from(p.clock.rate.denom));

            let end_date = (p.driver.finish - p.driver.signal) as f64 / 1000.;

            pop_front_push_back(&mut self.delay, max, delay);
            pop_front_push_back(&mut self.period, max, period);
            pop_front_push_back(&mut self.estimated, max, estimated);
            pop_front_push_back(&mut self.end_date, max, end_date);
        }

        fn clear(&mut self) {
            self.delay.clear();
            self.period.clear();
            self.estimated.clear();
            self.end_date.clear();
        }

        fn adjust_queues(&mut self, max: usize) {
            adjust_queue(&mut self.delay, max);
            adjust_queue(&mut self.period, max);
            adjust_queue(&mut self.estimated, max);
            adjust_queue(&mut self.end_date, max);
        }
    }

    pub struct Driver {
        last_profiling: Option<Profiling>,

        measurements: DriverMeasurements,
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

                measurements: DriverMeasurements::with_max_profilings(max_profilings),
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
            self.measurements.push(max_profilings, &profiling);

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
                                client.title = format!("{}/{}", follower.name, follower.id);
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

        pub const fn last_profiling(&self) -> Option<&Profiling> {
            self.last_profiling.as_ref()
        }

        pub fn name(&self) -> Option<&str> {
            self.last_profiling().map(|p| p.driver.name.as_str())
        }

        pub fn clear(&mut self) {
            self.measurements.clear();
            self.followers.clear();
        }

        pub fn adjust_queues(&mut self, max_profilings: usize) {
            self.measurements.adjust_queues(max_profilings);
            for follower in self.followers.values_mut() {
                follower.measurements.adjust_queues(max_profilings);
            }
        }

        pub fn delay(&self) -> PlotPoints<'_> {
            generate_plot_points(self.measurements.delay())
        }

        pub fn period(&self) -> PlotPoints<'_> {
            generate_plot_points(self.measurements.period())
        }

        pub fn estimated(&self) -> PlotPoints<'_> {
            generate_plot_points(self.measurements.estimated())
        }

        pub fn end_date(&self) -> PlotPoints<'_> {
            generate_plot_points(self.measurements.end_date())
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
    refresh_this_frame: Option<bool>,
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
            refresh_this_frame: None,
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
                if self.refresh_this_frame.is_none() {
                    self.refresh_this_frame = Some(false);
                }
                return;
            }
        }

        self.refresh_this_frame = Some(true);

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
            if let Some(last) = driver.last_profiling() {
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

        if ui.ctx().will_discard() {
            return;
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
                        format!("{name}: {:.3}us\nProcess cycle: {:.0}", value.y, value.x)
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

            if reset { plot.reset() } else { plot }
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
                    ui.line(egui_plot::Line::new(name, plot_points));
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
                ui.line(egui_plot::Line::new("Driver End Date", driver.end_date()));
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
                            ui.line(egui_plot::Line::new(client.title(), measurement(client)));
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

        fn draw_chart(driver: &Driver, refresh: bool, ui: &mut egui::Ui) {
            use egui_plot::{Bar, BarChart};

            let mut wait = Vec::with_capacity(1 + driver.n_clients());
            let mut busy = Vec::with_capacity(1 + driver.n_clients());
            let mut y_labels = Vec::with_capacity(1 + driver.n_clients());

            for (i, nb) in driver
                .clients()
                .map(|f| f.last_profiling())
                .chain(std::iter::once(
                    driver.last_profiling().map(|lp| &lp.driver),
                )) // NodeBlock of the driver
                .flatten()
                .enumerate()
            {
                wait.push(Bar::new(i as f64, (nb.awake - nb.signal) as f64 / 1000.).horizontal());
                busy.push(Bar::new(i as f64, (nb.finish - nb.awake) as f64 / 1000.).horizontal());

                let label = if nb.name.len() <= 15 {
                    format!("{} ({})", &nb.name, nb.id)
                } else {
                    format!("{}... ({})", &nb.name[0..15], nb.id)
                };

                y_labels.push(label);
            }

            ui.set_width(ui.available_width());

            // The plot is more readable when the bounds are updated less frequently
            // Keep the max bound equal to the max x of the last 60 updates
            let (prev_bound, mut counter) = ui
                .data(|d| d.get_temp::<(f64, u8)>(ui.id()))
                .unwrap_or((0., 0));

            let update_bound = counter >= 60;

            let res = Plot::new("Chart")
                .height(f32::max((y_labels.len() * 45) as f32, 115.))
                .allow_drag(false)
                .allow_zoom(false)
                .allow_scroll(false)
                .allow_boxed_zoom(false)
                .allow_axis_zoom_drag(false)
                .show_grid(egui::Vec2b::new(true, false))
                .set_margin_fraction(egui::Vec2::ZERO)
                .include_x(-0.5) // Left side margin
                .include_x(if update_bound { 0. } else { prev_bound })
                .include_y(-0.8) // 0.8 Y margin
                .include_y(y_labels.len() as f64 - 0.2) // 0.8 Y margin
                .y_grid_spacer({
                    let n_labels = y_labels.len();
                    move |_| {
                        // Always show all labels
                        (0..n_labels)
                            .map(|i| egui_plot::GridMark {
                                step_size: n_labels as f64,
                                value: i as f64,
                            })
                            .collect()
                    }
                })
                .x_axis_formatter(|grid_mark, _| format!("{} us", grid_mark.value))
                .y_axis_formatter(move |grid_mark, _| {
                    if grid_mark.value.is_sign_positive()
                        && (grid_mark.value as usize) < y_labels.len()
                        && grid_mark.value % 1. == 0.
                    {
                        y_labels[grid_mark.value as usize].clone()
                    } else {
                        String::new()
                    }
                })
                .label_formatter(|_, p| format!("{:.0} us", p.x))
                .legend(
                    egui_plot::Legend::default()
                        .position(egui_plot::Corner::LeftTop)
                        .text_style(egui::TextStyle::Small),
                )
                .show(ui, |plot_ui| {
                    let wait = BarChart::new("Waiting", wait)
                        .element_formatter(Box::new(|b, _| format!("Waiting took {} us", b.value)));

                    let busy = BarChart::new("Busy", busy)
                        .stack_on(&[&wait])
                        .element_formatter(Box::new(|b, _| {
                            format!("Processing took {} us", b.value)
                        }));

                    plot_ui.bar_chart(wait);
                    plot_ui.bar_chart(busy);
                });

            let plot_bound = *res.transform.bounds().range_x().end();

            let new_bound = if update_bound || prev_bound < plot_bound {
                // Reset the counter if it's time to update or if the data forces the bounds to expand
                if refresh {
                    counter = 0;
                }
                plot_bound
            } else {
                if refresh {
                    counter += 1;
                }
                prev_bound
            };

            ui.data_mut(|d| d.insert_temp(ui.id(), (new_bound, counter)));
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
            }

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
                if let Some(p) = driver.last_profiling() {
                    let keep = ui.horizontal(|ui| {
                        let keep = !ui.small_button("Delete").clicked();
                        if let Some(p) = driver.last_profiling() {
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

                    if ui.ctx().will_discard() {
                        return keep;
                    }

                    egui::CollapsingHeader::new("Chart").id_salt(id).show(ui, |ui| {
                        draw_chart(driver, self.refresh_this_frame.unwrap_or(false), ui);
                    });

                    ui.separator();

                    keep
                } else {
                    true
                }
            });
        });

        self.refresh_this_frame = None;
    }
}
