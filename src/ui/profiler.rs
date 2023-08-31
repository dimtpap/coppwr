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

use std::collections::{hash_map::Entry, HashMap};

use eframe::egui::{
    self,
    plot::{self, Plot, PlotPoints},
};

use crate::backend::pods::profiler::{Clock, Info, NodeBlock, Profiling};

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
mod data {
    use std::collections::{btree_map::Entry, BTreeMap, VecDeque};

    use eframe::egui::plot::PlotPoints;

    use crate::backend::pods::profiler::{NodeBlock, Profiling};

    fn pop_front_push_back<T>(queue: &mut VecDeque<T>, max: usize, value: T) {
        if queue.len() + 1 > max {
            queue.pop_front();
        }

        queue.push_back(value);
    }

    fn generate_plot_points(points: impl Iterator<Item = f64>) -> PlotPoints {
        PlotPoints::from_iter(points.enumerate().map(|(i, x)| [i as f64, x]))
    }

    struct ClientMeasurement {
        end_date: f64,
        scheduling_latency: f64,
        duration: f64,
    }

    impl ClientMeasurement {
        fn empty() -> Self {
            Self {
                end_date: f64::NAN,
                scheduling_latency: f64::NAN,
                duration: f64::NAN,
            }
        }

        fn new(follower: &NodeBlock, driver: &NodeBlock) -> Self {
            Self {
                end_date: (follower.finish - driver.signal) as f64,
                scheduling_latency: (follower.awake - follower.signal) as f64,
                duration: (follower.finish - follower.awake) as f64,
            }
        }
    }

    pub struct Client {
        title: String,
        measurements: VecDeque<ClientMeasurement>,

        // Position of last non-empty profiling that was added.
        // When this reaches 0 every profiling is empty indicating
        // that this follower has no statistics to show
        last_non_empty_pos: usize,
    }

    impl Client {
        fn new(title: String, max_profilings: usize) -> Self {
            Self {
                title,
                measurements: VecDeque::with_capacity(max_profilings),

                last_non_empty_pos: max_profilings,
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

            self.last_non_empty_pos = self.measurements.len();
        }

        fn add_empty_measurement(&mut self, max_profilings: usize) {
            pop_front_push_back(
                &mut self.measurements,
                max_profilings,
                ClientMeasurement::empty(),
            );

            self.last_non_empty_pos -= 1;
        }

        fn is_empty(&self) -> bool {
            self.last_non_empty_pos == 0
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
    }

    impl Driver {
        pub fn with_max_profilings(max_profilings: usize) -> Self {
            Self {
                last_profiling: None,

                measurements: VecDeque::with_capacity(max_profilings),
                followers: BTreeMap::new(),
            }
        }

        pub fn add_profiling(&mut self, profiling: Profiling, max_profilings: usize) {
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

            // Add new followers
            for follower in &profiling.followers {
                if let Entry::Vacant(e) = self.followers.entry(follower.id) {
                    e.insert(Client::new(
                        format!("{}/{}", follower.name, follower.id),
                        max_profilings,
                    ))
                    .add_measurement(
                        follower,
                        &profiling.driver,
                        max_profilings,
                    );
                }
            }

            self.last_profiling = Some(profiling);
        }

        pub fn last_profling(&self) -> Option<&Profiling> {
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

        pub fn clients(&self) -> impl Iterator<Item = &Client> + '_ {
            self.followers.values()
        }
    }
}

use data::{Client, Driver};

pub struct Profiler {
    max_profilings: usize,
    drivers: HashMap<i32, Driver>,
    selected_driver_id: Option<i32>,
    pause: bool,
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
        }
    }

    pub fn add_profilings(&mut self, profilings: Vec<Profiling>) {
        if self.pause {
            return;
        }

        for driver in self.drivers.values_mut() {
            driver.adjust_queues(self.max_profilings);
        }

        for p in profilings {
            match self.drivers.entry(p.driver.id) {
                Entry::Occupied(mut e) => {
                    e.get_mut().add_profiling(p, self.max_profilings);
                }
                Entry::Vacant(e) => {
                    e.insert(Driver::with_max_profilings(self.max_profilings))
                        .add_profiling(p, self.max_profilings);
                }
            }
        }
    }

    pub fn draw_profiler(&mut self, ui: &mut egui::Ui) {
        if ui
            .small_button("Reset")
            .on_hover_text("Clear all profiling data")
            .clicked()
        {
            self.drivers.clear();
            self.max_profilings = 250;
            self.selected_driver_id = None;
            self.pause = false;
            return;
        }

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
                        .map_or_else(|| format!("Unnamed driver {id}"), str::to_string);

                    ui.selectable_value(&mut self.selected_driver_id, Some(*id), name);
                }
            });

            driver
        }) else {
            ui.label("Select a driver to view profiling info");
            return;
        };

        ui.label(format!("Driver ID: {id}"));

        if let Some(last) = driver.last_profling() {
            let info = &last.info;
            let followers = last.followers.len();
            ui.label(format!(
                "Last profiling info\nTotal profiler samples: {} | Xruns: {} | Follower nodes: {}\nQuantum: {} | CPU Load: {} {} {}",
                info.counter, info.xrun_count, followers, last.clock.duration * i64::from(last.clock.rate.num), info.cpu_load_fast, info.cpu_load_medium, info.cpu_load_slow));
        }

        if ui.horizontal(|ui| {
            ui.label("Profilings");
            ui.add(egui::widgets::DragValue::new(&mut self.max_profilings).clamp_range(1..=1_000_000))
                .on_hover_text("Number of profiler samples to keep in memory. Very big values will slow down the application.");

            let clear = ui.button("Clear driver samples").clicked();

            ui.toggle_value(&mut self.pause, "Pause");

            clear
        }).inner {
            self.drivers.get_mut(&id).unwrap().clear();
            return;
        }

        fn profiler_plot(ui: &mut egui::Ui, heading: &str, id: &str, max_x: usize) -> Plot {
            let reset = ui
                .horizontal(|ui| {
                    ui.heading(heading);
                    ui.small_button("Reset").clicked()
                })
                .inner;

            let plot = Plot::new(id)
                .clamp_grid(true)
                .legend(plot::Legend::default())
                .allow_zoom(plot::AxisBools::new(true, false))
                .allow_drag(plot::AxisBools::new(true, false))
                .label_formatter(|name, value| {
                    if name.is_empty() {
                        String::new()
                    } else {
                        format!("{name}: {:.0} us\nProcess cycle: {:.0}", value.y, value.x)
                    }
                })
                .x_axis_formatter(move |x, _| {
                    if x.is_sign_negative() || x > max_x as f64 || x % 1. != 0. {
                        String::new()
                    } else {
                        format!("Process cycle {x:.0}")
                    }
                })
                .y_axis_formatter(|y, _| {
                    if y.is_sign_negative() {
                        String::new()
                    } else {
                        format!("{y} us")
                    }
                });

            if reset {
                plot.reset()
            } else {
                plot
            }
        }

        ui.separator();

        ui.columns(2, |ui| {
            profiler_plot(
                &mut ui[0],
                "Driver Timing",
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
                    ui.line(plot::Line::new(plot_points).name(name));
                }
            });

            profiler_plot(
                &mut ui[1],
                "Driver End Date",
                "driver_end_date",
                self.max_profilings,
            )
            .height(ui[1].available_height() / 2.)
            .show(&mut ui[1], |ui| {
                ui.line(plot::Line::new(driver.end_date()).name("Driver End Date"));
            });
        });

        ui.separator();

        ui.columns(3, |ui| {
            for (i, (heading, id, measurement)) in [
                (
                    "Clients End Date",
                    "clients_end_date",
                    Client::end_date as fn(&Client) -> PlotPoints,
                ),
                (
                    "Clients Scheduling Latency",
                    "clients_scheduling_latency",
                    Client::scheduling_latency,
                ),
                ("Clients Duration", "clients_duration", Client::duration),
            ]
            .into_iter()
            .enumerate()
            {
                profiler_plot(&mut ui[i], heading, id, self.max_profilings).show(
                    &mut ui[i],
                    |ui| {
                        for client in driver.clients() {
                            ui.line(plot::Line::new(measurement(client)).name(client.title()));
                        }
                    },
                );
            }
        });
    }

    pub fn draw_process_viewer(&mut self, ui: &mut egui::Ui) {
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

        ui.separator();

        fn draw_node_block(
            block: &NodeBlock,
            clock: &Clock,
            info: &Info,
            driver: bool,
            ui: &mut egui::Ui,
        ) {
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

        self.drivers.retain(|id, driver| {
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
                    .num_columns(9)
                    .show(ui, |ui| {
                        ui.label("ID");
                        ui.label("Name");
                        ui.label("Quantum");
                        ui.label("Rate");
                        ui.label("Waiting").on_hover_text("Time elapsed between when the node was ready to start processing and when it actually started processing");
                        ui.label("Busy").on_hover_text("Time between when the node started processing and when it finished and woke up the next nodes in the graph");
                        ui.label("Waiting/Quantum").on_hover_text("A measure of the graph load");
                        ui.label("Busy/Quantum").on_hover_text("A measure of the load of the driver/node");
                        ui.label("Xruns");
                        ui.end_row();
                        if let Some(p) = driver.last_profling() {
                            draw_node_block(&p.driver, &p.clock, &p.info, true, ui);
                            ui.end_row();

                            for nb in &p.followers {
                                draw_node_block(nb, &p.clock, &p.info, false, ui);
                                ui.end_row();
                            }
                        }
                    });
                });
            });
            ui.separator();

            keep
        });
    }
}
