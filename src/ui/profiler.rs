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

use std::collections::{hash_map::Entry, HashMap, VecDeque};

use eframe::egui::{
    self,
    plot::{self, Plot, PlotPoints},
};

use crate::backend::pods::profiler::{Clock, Info, NodeBlock, Profiling};

mod driver {
    use std::collections::VecDeque;

    use eframe::egui::plot::PlotPoints;

    use crate::backend::pods::profiler::Profiling;

    pub struct Driver {
        profilings: VecDeque<Profiling>,
        name: Option<String>,
    }

    impl Driver {
        pub fn with_max_profilings(max_profilings: usize) -> Self {
            Self {
                profilings: VecDeque::with_capacity(max_profilings),
                name: None,
            }
        }

        pub fn add_profiling(&mut self, profiling: Profiling, max_profilings: usize) {
            match &mut self.name {
                Some(name) => {
                    if *name != profiling.driver.name {
                        *name = profiling.driver.name.clone();
                    }
                }
                None => self.name = Some(profiling.driver.name.clone()),
            }

            if self.profilings.capacity() < max_profilings {
                self.profilings
                    .reserve(max_profilings - self.profilings.capacity());
            } else if self.profilings.len() > max_profilings {
                self.profilings
                    .drain(0..(self.profilings.len() - max_profilings));
                self.profilings.shrink_to(max_profilings);
            }

            if self.profilings.len() + 1 > max_profilings {
                self.profilings.pop_front();
            }

            self.profilings.push_back(profiling);
        }

        pub fn profiling_at(&self, i: usize) -> &Profiling {
            &self.profilings[i]
        }

        pub fn profilings(&self) -> &VecDeque<Profiling> {
            &self.profilings
        }

        pub fn name(&self) -> Option<&String> {
            self.name.as_ref()
        }

        pub fn clear(&mut self) {
            self.profilings.clear();
        }

        fn generate_plot_points(&self, measurement: fn(&Profiling) -> f64) -> PlotPoints {
            // Using from_ys would require 2 calls to .collect()
            PlotPoints::from_parametric_callback(
                |x| {
                    let x = x.floor();
                    (x, measurement(&self.profiling_at(x as usize)))
                },
                0f64..self.profilings().len() as f64,
                self.profilings().len(),
            )
        }

        pub fn delay(&self) -> PlotPoints {
            self.generate_plot_points(|p| {
                (p.clock.delay * 1_000_000) as f64 / f64::from(p.clock.rate.denom)
            })
        }

        pub fn period(&self) -> PlotPoints {
            self.generate_plot_points(|p| ((p.driver.signal - p.driver.prev_signal) / 1000) as f64)
        }

        pub fn estimated(&self) -> PlotPoints {
            self.generate_plot_points(|p| {
                (p.clock.duration * 1_000_000) as f64
                    / (p.clock.rate_diff * f64::from(p.clock.rate.denom))
            })
        }

        pub fn end_date(&self) -> PlotPoints {
            self.generate_plot_points(|p| ((p.driver.finish - p.driver.signal) / 1000) as f64)
        }
    }
}

use driver::Driver;

pub struct Profiler {
    max_profilings: usize,
    drivers: HashMap<i32, Driver>,
    selected_driver: Option<(i32, String)>,
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
            selected_driver: None,
            pause: false,
        }
    }

    pub fn add_profilings(&mut self, profilings: Vec<Profiling>) {
        if self.pause {
            return;
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
            self.selected_driver = None;
            self.pause = false;
            return;
        }

        egui::ComboBox::from_label("Driver")
            .selected_text(
                self.selected_driver
                    .as_ref()
                    .map_or("Select a driver", |(_, name)| name.as_str()),
            )
            .show_ui(ui, |ui| {
                for (id, driver) in &self.drivers {
                    let Some(name) = driver.name() else {
                        continue;
                    };
                    ui.selectable_value(
                        &mut self.selected_driver,
                        Some((*id, String::from(name))),
                        name,
                    );
                }
            });

        let driver = if let Some((id, _)) = self.selected_driver {
            ui.label(format!("Driver ID: {id}"));
            self.drivers.get_mut(&id).unwrap()
        } else {
            ui.label("Select a driver to view profiling info");
            return;
        };

        if let Some(last) = driver.profilings().back() {
            let info = &last.info;
            let followers = last.followers.len();
            ui.label(format!(
                "Last profiling info\nTotal profiler samples: {} | Xruns: {} | Follower nodes: {}\nQuantum: {} | CPU Load: {} {} {}",
                info.counter, info.xrun_count, followers, last.clock.duration * i64::from(last.clock.rate.num), info.cpu_load_fast, info.cpu_load_medium, info.cpu_load_slow));
        }

        ui.horizontal(|ui| {
            ui.label("Profilings");
            ui.add(egui::widgets::DragValue::new(&mut self.max_profilings).clamp_range(1..=1_000_000))
                .on_hover_text("Number of profiler samples to keep in memory. Very big values will slow down the application.");

            if ui.button("Clear driver samples").clicked() {
                driver.clear();
            }

            ui.toggle_value(&mut self.pause, "Pause");
        });

        fn profiler_plot_heading(heading: &str, ui: &mut egui::Ui) -> bool {
            ui.horizontal(|ui| {
                ui.heading(heading);
                ui.small_button("Reset").clicked()
            })
            .inner
        }

        fn profiler_plot(id: &str, max_x: usize, reset: bool) -> Plot {
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
                "driver_timing",
                self.max_profilings,
                profiler_plot_heading("Driver timing", &mut ui[0]),
            )
            .height(ui[0].available_height() / 2.)
            .show(&mut ui[0], |ui| {
                for (name, plot_points) in [
                    ("Driver Delay", driver.delay()),
                    ("Period", driver.period()),
                    ("Estimated", driver.estimated()),
                ]
                .into_iter()
                {
                    ui.line(plot::Line::new(plot_points).name(name));
                }
            });

            profiler_plot(
                "driver_end_date",
                self.max_profilings,
                profiler_plot_heading("Driver end date", &mut ui[1]),
            )
            .height(ui[1].available_height() / 2.)
            .show(&mut ui[1], |ui| {
                ui.line(plot::Line::new(driver.end_date()).name("Driver end date"));
            });
        });

        ui.separator();

        fn per_client_plot(
            id: &str,
            max_x: usize,
            reset: bool,
            profilings: &VecDeque<Profiling>,
            measurement: fn(&NodeBlock, &NodeBlock) -> i64,
            ui: &mut egui::Ui,
        ) {
            let Some(followers) = profilings.back().map(|p| &p.followers) else {
                return;
            };
            profiler_plot(id, max_x, reset).show(ui, |ui| {
                for node in followers {
                    ui.line(
                        plot::Line::new(PlotPoints::from_parametric_callback(
                            |x| {
                                let x = x.floor();
                                if let Some(f) = profilings[x as usize]
                                    .followers
                                    .iter()
                                    .find(|f| f.id == node.id)
                                {
                                    let val = measurement(f, &profilings[x as usize].driver) as f64;
                                    if val > 0. {
                                        (x, val / 1000.)
                                    } else {
                                        (f64::NAN, f64::NAN)
                                    }
                                } else {
                                    (f64::NAN, f64::NAN)
                                }
                            },
                            0f64..profilings.len() as f64,
                            profilings.len(),
                        ))
                        .name(format!("{}/{}", &node.name, node.id)),
                    );
                }
            });
        }

        ui.columns(3, |ui| {
            // (Follower block, driver block)
            let measurements: [fn(&NodeBlock, &NodeBlock) -> i64; 3] = [
                |nb, d| nb.finish - d.signal,
                |nb, _| nb.awake - nb.signal,
                |nb, _| nb.finish - nb.awake,
            ];
            for (i, ((heading, id), measurement)) in [
                ("Clients End Date", "clients_end_date"),
                ("Clients Scheduling Latency", "clients_scheduling_latency"),
                ("Clients Duration", "clients_duration"),
            ]
            .into_iter()
            .zip(measurements)
            .enumerate()
            {
                per_client_plot(
                    id,
                    self.max_profilings,
                    profiler_plot_heading(heading, &mut ui[i]),
                    driver.profilings(),
                    measurement,
                    &mut ui[i],
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
            self.selected_driver = None;
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
                if let Some(p) = driver.profilings().back() {
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
                        if let Some(p) = driver.profilings().back() {
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
