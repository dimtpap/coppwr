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
    collections::{HashMap, HashSet, VecDeque},
    rc::Rc,
};

use eframe::egui;
use egui::emath::TSTransform;
use egui_snarl::{
    InPin, InPinId, NodeId, OutPin, OutPinId, Snarl,
    ui::{PinInfo, SnarlPin, SnarlStyle, WireLayer},
};
use pipewire::{spa::param::format::MediaType, types::ObjectType};

use crate::{
    backend::{self, Request},
    ui::{
        globals_store::{Global, ObjectData},
        util::persistence::PersistentView,
    },
};

struct Port {
    id: u32,
    name: String,
    global: Rc<RefCell<Global>>,
}

impl Port {
    fn snarl_pin_info(&self) -> PinInfo {
        let global = self.global.borrow();

        let media_type = match global.object_data() {
            ObjectData::Port(media_type) => Some(media_type),
            ObjectData::Other(ObjectType::Port) => None, // This is in case the media type has not be sent by PipeWire yet
            _ => panic!("Global referenced by a graph pin should be a port"),
        };

        let color = media_type.map_or(egui::Color32::GRAY, |media_type| match *media_type {
            MediaType::Audio => egui::Color32::BLUE,
            MediaType::Video => egui::Color32::YELLOW,
            MediaType::Application => egui::Color32::RED,
            MediaType::Binary => egui::Color32::GREEN,
            MediaType::Image => egui::Color32::ORANGE,
            _ => egui::Color32::GRAY,
        });

        PinInfo::circle().with_fill(color)
    }
}

struct Node {
    user_label: String,
    inputs: Vec<Port>,
    outputs: Vec<Port>,
    resize: bool,
    global: Rc<RefCell<Global>>,
}

impl Node {
    fn new(global: Rc<RefCell<Global>>) -> Self {
        let name = global.borrow().name().cloned().unwrap_or_default();
        Self {
            user_label: name,
            inputs: Vec::new(),
            outputs: Vec::new(),
            resize: false,
            global,
        }
    }
}

struct Viewer<'a, 'b> {
    sx: &'a backend::Sender,
    wires: &'b HashMap<(OutPinId, InPinId), u32>,
    transform: Option<TSTransform>,
}

impl egui_snarl::ui::SnarlViewer<Node> for Viewer<'_, '_> {
    fn title(&mut self, _: &Node) -> String {
        String::new()
    }

    fn show_header(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut egui::Ui,
        _scale: f32,
        snarl: &mut Snarl<Node>,
    ) {
        let node = snarl
            .get_node_mut(node)
            .expect("snarl requested header of non-existent node");

        ui.label(node.global.borrow().id().to_string());

        node.resize = egui::TextEdit::singleline(&mut node.user_label)
            .desired_width(0.0)
            .clip_text(false)
            .show(ui)
            .response
            .changed();
    }

    fn has_footer(&mut self, _: &Node) -> bool {
        true
    }

    fn outputs(&mut self, node: &Node) -> usize {
        node.outputs.len()
    }

    fn inputs(&mut self, node: &Node) -> usize {
        node.inputs.len()
    }

    fn current_transform(
        &mut self,
        trasnform: &mut egui::emath::TSTransform,
        _snarl: &mut Snarl<Node>,
    ) {
        if let Some(initial_transform) = self.transform.take() {
            *trasnform = initial_transform;
        } else {
            self.transform = Some(*trasnform);
        }
    }

    fn show_footer(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut egui::Ui,
        scale: f32,
        snarl: &mut Snarl<Node>,
    ) {
        if !snarl
            .get_node_info(node)
            .expect("snarl requested footer of non-existent node")
            .open
        {
            return;
        }

        let node = snarl.get_node_mut(node).unwrap();

        // The global should expand to fill the space around it if the snarl frame
        // is wide enough. This can be done by making the global justified, which
        // causes this problem:
        // 1. Node and global are drawn
        // 2. Something in the global is expanded, causing the node to widen
        // 3. Expanded elements are collapsed
        // 4. Global stays wide, making the node stay wide, wasting space
        //
        // To solve this, store the minimum width required by the global and the node.
        // If the global width is less than the node width, the global should expand
        // to exactly the width of the node

        let min_global_width_key = ui.id().with("min_global_width");
        let min_node_width_key = ui.id().with("min_node_width");

        if node.resize {
            // Redo size calculations
            ui.data_mut(|d| {
                d.remove_temp::<f32>(min_global_width_key);
                d.remove_temp::<f32>(min_node_width_key);
            });

            node.resize = false;
        }

        let min_global_width: Option<f32> = ui.data(|d| d.get_temp(min_global_width_key));
        let current_node_width = ui.max_rect().width() / scale;

        let global_width = egui::CollapsingHeader::new("Details")
            .default_open(true)
            .show_unindented(ui, |ui| {
                let mut ui_builder = egui::UiBuilder::new();

                // Only after the global has been drawn we can know the final node width
                let min_node_width = if min_global_width.is_some() {
                    ui.data_mut(|d| {
                        let min_node_width =
                            d.get_temp_mut_or(min_node_width_key, current_node_width);
                        Some(*min_node_width)
                    })
                } else {
                    ui_builder.sizing_pass = true;
                    ui.ctx().request_discard(format!(
                        "graph node {} sizing",
                        node.global.borrow().id()
                    ));
                    None
                };

                ui.scope_builder(ui_builder, |ui| {
                    egui::ScrollArea::vertical()
                        .min_scrolled_height(450. * scale)
                        .max_height(450. * scale)
                        .show(ui, |ui| {
                            let mut layout = egui::Layout::top_down(egui::Align::Min);

                            if !ui.is_sizing_pass() {
                                // The part of the node drawn by snarl includes the port names which may make
                                // it wider than the width of the global. If that's the case, the global
                                // should expand to fill the unused space. If that is not the case the global
                                // should stay as narrow as possible and expand the outer UI only when needed
                                if let Some((min_node_width, _)) =
                                    Option::zip(min_node_width, min_global_width)
                                        .filter(|&(n, g)| n > g)
                                {
                                    layout.cross_justify = true;
                                    ui.set_max_width(min_node_width * scale);
                                } else {
                                    ui.set_max_width(450. * scale);
                                }
                            }

                            ui.with_layout(layout, |ui| {
                                // Text should wrap when sizing. Otherwise the followring is possible
                                // 1. Node appears and some collapsing header is open, showing some wide text
                                // 2. The wide text causes the calculcated width to be larger than needed
                                // 3. The collapsing header is closed, but because of 2. the node stays wide
                                if !ui.is_sizing_pass() {
                                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                                }
                                node.global.borrow_mut().show(ui, true, self.sx);
                            });
                        });
                })
                .response
                .rect
                .width()
                    / scale
            })
            .body_returned;

        if let Some(global_width) = global_width {
            if min_global_width.is_none() {
                ui.data_mut(|d| {
                    d.insert_temp(min_global_width_key, global_width);
                });
            }
        }
    }

    fn show_input(
        &mut self,
        pin: &InPin,
        ui: &mut egui::Ui,
        _scale: f32,
        snarl: &mut Snarl<Node>,
    ) -> impl SnarlPin + 'static {
        let node = snarl
            .get_node(pin.id.node)
            .expect("snarl requested showing of pin not belonging to any node");

        let port = &node.inputs[pin.id.input];

        if snarl.get_node_info(pin.id.node).unwrap().open {
            ui.label(&port.name);
        }

        port.snarl_pin_info()
    }

    fn show_output(
        &mut self,
        pin: &OutPin,
        ui: &mut egui::Ui,
        _scale: f32,
        snarl: &mut Snarl<Node>,
    ) -> impl SnarlPin + 'static {
        let node = snarl
            .get_node(pin.id.node)
            .expect("snarl requested showing of pin not belonging to any node");

        let port = &node.outputs[pin.id.output];

        if snarl.get_node_info(pin.id.node).unwrap().open {
            ui.label(&port.name);
        }

        port.snarl_pin_info()
    }

    fn connect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<Node>) {
        let Some(out) = snarl.get_node(from.id.node) else {
            eprintln!("snarl requested connection from port of non-existent node");
            return;
        };

        let Some(inp) = snarl.get_node(to.id.node) else {
            eprintln!("snarl requested connection to port of non-existent node");
            return;
        };

        self.sx
            .send(Request::CreateObject(
                ObjectType::Link,
                "link-factory".to_owned(),
                vec![
                    (
                        "link.output.port".to_owned(),
                        out.outputs[from.id.output].id.to_string(),
                    ),
                    (
                        "link.input.port".to_owned(),
                        inp.inputs[to.id.input].id.to_string(),
                    ),
                    ("object.linger".to_owned(), "true".to_owned()),
                ],
            ))
            .ok();
    }

    fn disconnect(&mut self, from: &OutPin, to: &InPin, _snarl: &mut Snarl<Node>) {
        let Some(&link_id) = self.wires.get(&(from.id, to.id)) else {
            eprintln!("snarl requested destruction of non-existent link");
            return;
        };

        self.sx.send(Request::DestroyObject(link_id)).ok();
    }

    // Make secondary-clicking on ports do nothing
    fn drop_inputs(&mut self, _pin: &InPin, _snarl: &mut Snarl<Node>) {}
    fn drop_outputs(&mut self, _pin: &OutPin, _snarl: &mut Snarl<Node>) {}
}

pub struct Graph {
    snarl: Snarl<Node>,
    nodes: HashMap<u32, NodeId>,
    wires: HashMap<(OutPinId, InPinId), u32>,
    ports: HashSet<u32>,

    unpositioned: HashSet<NodeId>,

    transform: TSTransform,

    restored_positions: Option<HashMap<String, VecDeque<egui::Pos2>>>,
    restored_transform: Option<TSTransform>,
}

impl Graph {
    pub fn new() -> Self {
        Self {
            snarl: Snarl::new(),
            nodes: HashMap::new(),
            wires: HashMap::new(),
            ports: HashSet::new(),

            unpositioned: HashSet::new(),

            transform: TSTransform::IDENTITY,

            restored_positions: None,
            restored_transform: None,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, sx: &crate::backend::Sender) {
        let style = SnarlStyle {
            min_scale: Some(0.5),
            max_scale: Some(1.5),
            bg_pattern: Some(egui_snarl::ui::BackgroundPattern::grid(
                egui::vec2(50., 50.),
                0.,
            )),
            header_frame: Some(
                egui::Frame::window(ui.style())
                    .multiply_with_opacity(0.5)
                    .shadow(egui::Shadow::NONE),
            ),
            node_frame: Some(
                egui::Frame::window(ui.style())
                    .multiply_with_opacity(0.85)
                    .shadow(egui::Shadow::NONE),
            ),
            pin_placement: Some(egui_snarl::ui::PinPlacement::Edge),
            wire_layer: Some(WireLayer::AboveNodes),
            ..SnarlStyle::default()
        };

        if !self.unpositioned.is_empty() {
            const NODE_SPACING: egui::Vec2 = egui::vec2(250f32, 150f32);

            let mut next_sources_pos = egui::pos2(-NODE_SPACING.x, 0.);
            let mut next_others_pos = egui::Pos2::ZERO;
            let mut next_sinks_pos = egui::pos2(NODE_SPACING.x, 0.);

            for (node_id, pos, _) in self.snarl.nodes_pos_ids() {
                if self.unpositioned.contains(&node_id) {
                    continue;
                }

                // Determine next available position for this node's kind
                for next in [
                    &mut next_sources_pos,
                    &mut next_others_pos,
                    &mut next_sinks_pos,
                ] {
                    if (pos.x - 50f32..=pos.x + 50f32).contains(&next.x)
                        && (next.y..next.y + NODE_SPACING.y).contains(&pos.y)
                    {
                        next.y += NODE_SPACING.y;
                        break;
                    }
                }
            }

            // Position unpositioned nodes
            for node in self.unpositioned.drain() {
                let Some(node) = self.snarl.get_node_info_mut(node) else {
                    eprintln!("Tried to position node not in snarl");
                    continue;
                };

                let global = node.value.global.borrow();

                let ports = global.info().and_then(|info| {
                    info[2]
                        .1
                        .parse::<u32>()
                        .ok()
                        .zip(info[3].1.parse::<u32>().ok())
                });

                let new_pos = if let Some((inputs, outputs)) = ports {
                    if inputs == 0 && outputs == 0 {
                        &mut next_others_pos
                    } else if inputs == 0 {
                        &mut next_sources_pos
                    } else if outputs == 0 {
                        &mut next_sinks_pos
                    } else {
                        &mut next_others_pos
                    }
                } else {
                    &mut next_others_pos
                };

                node.pos = *new_pos;

                new_pos.y += NODE_SPACING.y;
            }
        }

        let mut viewer = Viewer {
            sx,
            wires: &mut self.wires,
            transform: self.restored_transform.take(),
        };

        self.snarl.show(&mut viewer, &style, "graph", ui);

        self.transform = viewer.transform.unwrap_or(self.transform);

        let controls_layer_id =
            egui::LayerId::new(ui.layer_id().order, ui.layer_id().id.with("controls"));
        ui.scope_builder(
            egui::UiBuilder::new()
                .layer_id(controls_layer_id)
                .max_rect(ui.max_rect().shrink(3.)),
            |ui| {
                egui::Frame::canvas(ui.style())
                    .shadow(egui::Shadow::NONE)
                    .inner_margin(egui::Margin::symmetric(5, 2))
                    .stroke(ui.style().visuals.window_stroke)
                    .show(ui, |ui| {
                        egui::CollapsingHeader::new("Controls")
                            .default_open(true)
                            .show_unindented(ui, |ui| {
                                ui.label(
                                    "Reset view: Double click\n\
                                    Pan: Click & Drag\n\
                                    Select nodes: Shift + Click & Drag\n\
                                    Deselect nodes: Ctrl+Shift + Click & Drag\n\
                                    Zoom: Ctrl & Scroll\n\
                                    Start linking: Click & Drag from port\n\
                                    Destroy link: Right click on link",
                                );
                            });
                    });
            },
        );

        ui.ctx().set_sublayer(ui.layer_id(), controls_layer_id);
    }

    pub fn add_node(&mut self, global: &Rc<RefCell<Global>>) {
        let pos = if let Some(name) = global.borrow().name() {
            self.restored_positions
                .as_mut()
                .and_then(|rp| rp.get_mut(name))
                .and_then(VecDeque::pop_front)
                .unwrap_or(egui::Pos2::ZERO)
        } else {
            egui::Pos2::ZERO
        };

        let node_id = self.snarl.insert_node(pos, Node::new(Rc::clone(global)));
        self.nodes.insert(global.borrow().id(), node_id);

        if pos == egui::Pos2::ZERO {
            self.unpositioned.insert(node_id);
        }
    }

    pub fn add_input_port(&mut self, global: &Rc<RefCell<Global>>) {
        let port_id = global.borrow().id();

        if !self.ports.insert(port_id) {
            return;
        }

        let Some(&node_id) = global
            .borrow()
            .parent_id()
            .and_then(|id| self.nodes.get(&id))
        else {
            return;
        };

        let Some(node) = self.snarl.get_node_mut(node_id) else {
            return;
        };

        node.inputs.push(Port {
            id: port_id,
            name: format!(
                "{} ({})",
                global.borrow().name().cloned().unwrap_or_default(),
                global.borrow().id()
            ),
            global: Rc::clone(global),
        });

        node.resize = true;
    }

    pub fn add_output_port(&mut self, global: &Rc<RefCell<Global>>) {
        let port_id = global.borrow().id();

        if !self.ports.insert(port_id) {
            return;
        }

        let Some(&node_id) = global
            .borrow()
            .parent_id()
            .and_then(|id| self.nodes.get(&id))
        else {
            return;
        };

        let Some(node) = self.snarl.get_node_mut(node_id) else {
            return;
        };

        node.outputs.push(Port {
            id: port_id,
            name: format!(
                "{} ({})",
                global.borrow().name().cloned().unwrap_or_default(),
                global.borrow().id()
            ),
            global: Rc::clone(global),
        });

        node.resize = true;
    }

    pub fn add_link(
        &mut self,
        output_node: u32,
        output_port: u32,
        input_node: u32,
        input_port: u32,
        link_id: u32,
    ) {
        let (Some(&out_node_id), Some(&in_node_id)) =
            (self.nodes.get(&output_node), self.nodes.get(&input_node))
        else {
            return;
        };

        let (Some(out_node), Some(in_node)) = (
            self.snarl.get_node(out_node_id),
            self.snarl.get_node(in_node_id),
        ) else {
            return;
        };

        let (Some(output_port), Some(input_port)) = (
            out_node
                .outputs
                .iter()
                .enumerate()
                .find_map(|(idx, p)| (p.id == output_port).then_some(idx)),
            in_node
                .inputs
                .iter()
                .enumerate()
                .find_map(|(idx, p)| (p.id == input_port).then_some(idx)),
        ) else {
            return;
        };

        let out_pin_id = OutPinId {
            node: out_node_id,
            output: output_port,
        };

        let in_pin_id = InPinId {
            node: in_node_id,
            input: input_port,
        };

        if self.snarl.connect(out_pin_id, in_pin_id) {
            self.wires.insert((out_pin_id, in_pin_id), link_id);
        }
    }

    pub fn remove_link(&mut self, id: u32) {
        self.wires.retain(|(out, inp), &mut link_id| {
            if link_id == id {
                self.snarl.disconnect(*out, *inp);
                false
            } else {
                true
            }
        });
    }

    pub fn remove_port(&mut self, node_id: u32, port_id: u32) {
        let Some(&node_id) = self.nodes.get(&node_id) else {
            return;
        };

        let Some(node) = self.snarl.get_node_mut(node_id) else {
            return;
        };

        if self.ports.remove(&port_id) {
            node.inputs.retain(|p| p.id != port_id);
            node.outputs.retain(|p| p.id != port_id);
        }
    }

    pub fn remove_node(&mut self, id: u32) {
        if let Some(node_id) = self.nodes.remove(&id) {
            self.wires.retain(|(out, inp), _| {
                if out.node == node_id || inp.node == node_id {
                    self.snarl.disconnect(*out, *inp);
                    false
                } else {
                    true
                }
            });
            self.unpositioned.remove(&node_id);
            self.snarl.remove_node(node_id);
        }
    }
}

#[cfg_attr(feature = "persistence", derive(serde::Serialize, serde::Deserialize))]
pub struct PersistentData {
    positions: HashMap<String, VecDeque<egui::Pos2>>,
    transform: TSTransform,
}

impl PersistentView for Graph {
    type Data = PersistentData;

    fn with_data(data: &Self::Data) -> Self {
        Self {
            restored_positions: Some(data.positions.clone()),
            restored_transform: Some(data.transform),
            ..Self::new()
        }
    }

    fn save_data(&self) -> Option<Self::Data> {
        let mut positions: HashMap<String, VecDeque<egui::Pos2>> = HashMap::new();

        for node_info in self.snarl.nodes_info() {
            let node = &node_info.value;

            if let Some(name) = node.global.borrow().name() {
                positions
                    .entry(name.clone())
                    .or_default()
                    .push_back(node_info.pos);
            }
        }

        if positions.is_empty() {
            None
        } else {
            Some(PersistentData {
                positions,
                transform: self.transform,
            })
        }
    }
}
