// Copyright 2023-2024 Dimitris Papaioannou <dimtpap@protonmail.com>
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
    borrow::Cow,
    cell::RefCell,
    collections::{BTreeMap, HashMap, VecDeque},
    rc::{Rc, Weak},
};

use eframe::egui;
use egui_node_graph::{
    AnyParameterId, DataTypeTrait, GraphEditorState, InputId, NodeDataTrait, NodeId, NodeResponse,
    OutputId, UserResponseTrait,
};
use pipewire::types::ObjectType;

use crate::{
    backend::{self, Request},
    ui::{globals_store::Global, persistence::PersistentView},
};

// Used to satisfy trait bounds that provide unneded features
#[derive(Debug, Default, Clone)]
struct NoOp;
impl egui_node_graph::WidgetValueTrait for NoOp {
    type Response = Self;
    type NodeData = GraphNode;
    type UserState = backend::Sender;

    fn value_widget(
        &mut self,
        _: &str,
        _: egui_node_graph::NodeId,
        _: &mut egui::Ui,
        _: &mut Self::UserState,
        _: &Self::NodeData,
    ) -> Vec<Self::Response> {
        Vec::new()
    }
}
impl egui_node_graph::UserResponseTrait for NoOp {}
impl egui_node_graph::NodeTemplateTrait for NoOp {
    type NodeData = GraphNode;
    type DataType = MediaType;
    type ValueType = Self;
    type CategoryType = ();
    type UserState = backend::Sender;

    fn node_finder_categories(&self, _: &mut Self::UserState) -> Vec<Self::CategoryType> {
        Vec::new()
    }

    fn node_finder_label(&self, _: &mut Self::UserState) -> std::borrow::Cow<str> {
        Cow::Borrowed("")
    }

    fn build_node(
        &self,
        _: &mut egui_node_graph::Graph<Self::NodeData, Self::DataType, Self::ValueType>,
        _: &mut Self::UserState,
        _: egui_node_graph::NodeId,
    ) {
    }

    fn node_graph_label(&self, _: &mut Self::UserState) -> String {
        String::new()
    }

    fn user_data(&self, _: &mut Self::UserState) -> Self::NodeData {
        GraphNode::NoOp
    }
}
impl egui_node_graph::NodeTemplateIter for NoOp {
    type Item = Self;
    fn all_kinds(&self) -> Vec<Self::Item> {
        Vec::new()
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum MediaType {
    Audio,
    Video,
    Midi,
    Unknown,
}

impl DataTypeTrait<backend::Sender> for MediaType {
    fn data_type_color(&self, _: &mut backend::Sender) -> egui::Color32 {
        match self {
            Self::Audio => egui::Color32::BLUE,
            Self::Video => egui::Color32::YELLOW,
            Self::Midi => egui::Color32::RED,
            Self::Unknown => egui::Color32::GRAY,
        }
    }

    fn name(&self) -> std::borrow::Cow<str> {
        match self {
            Self::Audio => Cow::Borrowed("Audio"),
            Self::Video => Cow::Borrowed("Video"),
            Self::Midi => Cow::Borrowed("MIDI"),
            Self::Unknown => Cow::Borrowed("Unknown"),
        }
    }
}

enum GraphNode {
    Node {
        media_type: MediaType,
        global: Weak<RefCell<Global>>,
    },
    NoOp,
}

impl GraphNode {
    fn new(media_type: MediaType, global: Weak<RefCell<Global>>) -> Self {
        Self::Node { media_type, global }
    }
}

impl NodeDataTrait for GraphNode {
    type DataType = MediaType;
    type Response = NoOp;
    type ValueType = NoOp;
    type UserState = backend::Sender;

    fn can_delete(
        &self,
        _: egui_node_graph::NodeId,
        _: &egui_node_graph::Graph<Self, Self::DataType, Self::ValueType>,
        _: &mut Self::UserState,
    ) -> bool {
        false
    }

    fn bottom_ui(
        &self,
        ui: &mut egui::Ui,
        _node_id: egui_node_graph::NodeId,
        _graph: &egui_node_graph::Graph<Self, Self::DataType, Self::ValueType>,
        sx: &mut Self::UserState,
    ) -> Vec<egui_node_graph::NodeResponse<Self::Response, Self>>
    where
        Self::Response: UserResponseTrait,
    {
        if let Self::Node { global, .. } = self {
            if let Some(global) = global.upgrade() {
                egui::CollapsingHeader::new("Details")
                    .default_open(true)
                    .show_unindented(ui, |ui| {
                        egui::Frame::central_panel(&egui::Style::default())
                            .inner_margin(egui::Margin::same(2.5))
                            .rounding(ui.visuals().noninteractive().rounding)
                            .show(ui, |ui| {
                                ui.set_max_width(500f32);
                                egui::ScrollArea::vertical()
                                    .min_scrolled_height(350f32)
                                    .max_height(350f32)
                                    .show(ui, |ui| {
                                        global.borrow_mut().show(ui, true, sx);
                                    });
                            });
                    });
            }
        };

        Vec::new()
    }
}

enum GraphItem {
    Node(NodeId),
    InputPort(InputId),
    OutputPort(OutputId),
    Link(OutputId, InputId),
}

impl From<NodeId> for GraphItem {
    fn from(value: NodeId) -> Self {
        Self::Node(value)
    }
}

impl From<InputId> for GraphItem {
    fn from(value: InputId) -> Self {
        Self::InputPort(value)
    }
}

impl From<OutputId> for GraphItem {
    fn from(value: OutputId) -> Self {
        Self::OutputPort(value)
    }
}

impl From<AnyParameterId> for GraphItem {
    fn from(value: AnyParameterId) -> Self {
        match value {
            AnyParameterId::Input(value) => Self::from(value),
            AnyParameterId::Output(value) => Self::from(value),
        }
    }
}

impl From<(OutputId, InputId)> for GraphItem {
    fn from((output, input): (OutputId, InputId)) -> Self {
        Self::Link(output, input)
    }
}

pub struct Graph {
    restored_positions: Option<HashMap<String, VecDeque<egui::Pos2>>>,

    editor: egui_node_graph::GraphEditorState<GraphNode, MediaType, NoOp, NoOp, backend::Sender>,
    responses: Vec<NodeResponse<NoOp, GraphNode>>,

    // Maps PipeWire global IDs to graph items
    graph_items: BTreeMap<u32, GraphItem>,
}

impl Graph {
    pub fn new() -> Self {
        Self {
            restored_positions: None,

            editor: GraphEditorState::default(),
            responses: Vec::new(),
            graph_items: BTreeMap::new(),
        }
    }

    pub fn add_node(&mut self, id: u32, global: &Rc<RefCell<Global>>) {
        if self.graph_items.get(&id).is_some() {
            return;
        }

        // TODO Use port params to get their media type and move this out of Nodes.
        let media_type =
            global
                .borrow()
                .props()
                .get("media.class")
                .map_or(MediaType::Unknown, |media_class| {
                    let media_class = media_class.to_lowercase();
                    if media_class.contains("audio") {
                        MediaType::Audio
                    } else if media_class.contains("video") {
                        MediaType::Video
                    } else if media_class.contains("midi") {
                        MediaType::Midi
                    } else {
                        MediaType::Unknown
                    }
                });

        let graph_id = self.editor.graph.add_node(
            global
                .borrow()
                .name()
                .cloned()
                .unwrap_or_else(|| format!("{id}")),
            GraphNode::new(media_type, Rc::downgrade(global)),
            |_, _| {},
        );

        self.responses.push(NodeResponse::CreatedNode(graph_id));

        self.graph_items.insert(id, graph_id.into());
    }

    fn port_graph_node_and_media_type(
        &self,
        id: u32,
        node_id: u32,
    ) -> Option<(&NodeId, MediaType)> {
        if self.graph_items.get(&id).is_some() {
            return None;
        }

        let Some(GraphItem::Node(node_id)) = self.graph_items.get(&node_id) else {
            return None;
        };

        if let GraphNode::Node { ref media_type, .. } =
            self.editor.graph.nodes.get(*node_id).unwrap().user_data
        {
            Some((node_id, *media_type))
        } else {
            unreachable!();
        }
    }

    pub fn add_input_port(&mut self, id: u32, node_id: u32, name: String) {
        let Some((node_id, media_type)) = self.port_graph_node_and_media_type(id, node_id) else {
            return;
        };

        let graph_id = self.editor.graph.add_wide_input_param(
            *node_id,
            name,
            media_type,
            NoOp,
            egui_node_graph::InputParamKind::ConnectionOnly,
            None,
            true,
        );

        self.graph_items.insert(id, graph_id.into());
    }

    pub fn add_output_port(&mut self, id: u32, node_id: u32, name: String) {
        let Some((node_id, media_type)) = self.port_graph_node_and_media_type(id, node_id) else {
            return;
        };

        let graph_id = self
            .editor
            .graph
            .add_output_param(*node_id, name, media_type);

        self.graph_items.insert(id, graph_id.into());
    }

    pub fn add_link(&mut self, id: u32, output_port_id: u32, input_port_id: u32) {
        if self.graph_items.get(&id).is_some() {
            return;
        }

        let Some((GraphItem::OutputPort(output), GraphItem::InputPort(input))) = self
            .graph_items
            .get(&output_port_id)
            .zip(self.graph_items.get(&input_port_id))
        else {
            return;
        };

        self.editor.graph.add_connection(*output, *input, 0);

        self.graph_items
            .insert(id, GraphItem::Link(*output, *input));
    }

    pub fn remove_item(&mut self, id: u32) {
        let Some(item) = self.graph_items.remove(&id) else {
            return;
        };

        match item {
            GraphItem::Node(node_id) => {
                self.responses.push(NodeResponse::DeleteNodeUi(node_id));
            }
            GraphItem::OutputPort(output_id) => self.editor.graph.remove_output_param(output_id),
            GraphItem::InputPort(input_id) => self.editor.graph.remove_input_param(input_id),
            GraphItem::Link(output_id, input_id) => {
                self.editor.graph.remove_connection(input_id, output_id);
            }
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, sx: &mut backend::Sender) {
        // Never show the node finder since nodes can't be created manually
        self.editor.node_finder = None;

        let reset_zoom = ui
            .horizontal(|ui| {
                if ui.button("Auto arrange").clicked() {
                    self.editor.node_positions.clear();
                    self.editor.node_order.clear();
                }

                ui.label("Zoom");
                ui.add(
                    egui::Slider::new(&mut self.editor.pan_zoom.zoom, 0.2..=2.0).max_decimals(2),
                );

                ui.button("Reset zoom").clicked()
            })
            .inner;
        ui.separator();

        const NODE_SPACING: egui::Vec2 = egui::vec2(200f32, 100f32);

        let mut next_outputs_only_pos = egui::Pos2::ZERO;
        let mut next_default_pos =
            egui::Pos2::new((ui.available_width() - NODE_SPACING.x) / 2., 0f32);
        let mut next_inputs_only_pos = egui::Pos2::new(
            ui.available_width() - NODE_SPACING.x - ui.style().spacing.window_margin.right,
            0f32,
        );

        for pos in self.editor.node_positions.values_mut() {
            // Adjust existing nodes' positions so that they're inside the drawable area
            pos.x = pos
                .x
                .clamp(0., f32::max(0., ui.available_width() - NODE_SPACING.x));
            pos.y = pos
                .y
                .clamp(0., f32::max(0., ui.available_height() - NODE_SPACING.y));

            // Determine next available position for this node's kind
            for next in [
                &mut next_inputs_only_pos,
                &mut next_default_pos,
                &mut next_outputs_only_pos,
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
        for (id, node) in &self.editor.graph.nodes {
            if self.editor.node_positions.contains_key(id) {
                continue;
            }

            let GraphNode::Node { ref global, .. } = node.user_data else {
                unreachable!();
            };

            self.editor.node_order.push(id);

            let mut ports = None;

            if let Some(global) = global.upgrade() {
                let global = global.borrow();

                if let Some(restored_positions) = &mut self.restored_positions {
                    if let Some(name) = global.props().get("node.name") {
                        if let Some(pos) = restored_positions
                            .get_mut(name)
                            .and_then(VecDeque::pop_front)
                        {
                            self.editor.node_positions.insert(id, pos);
                            return;
                        }
                    }
                }

                ports = global.info().and_then(|info| {
                    info[2]
                        .1
                        .parse::<u32>()
                        .ok()
                        .zip(info[3].1.parse::<u32>().ok())
                });
            };

            let pos = if let Some((inputs, outputs)) = ports {
                if outputs == 0 {
                    &mut next_inputs_only_pos
                } else if inputs == 0 {
                    &mut next_outputs_only_pos
                } else {
                    &mut next_default_pos
                }
            } else {
                &mut next_default_pos
            };

            self.editor.node_positions.insert(id, *pos);

            pos.y += NODE_SPACING.y;
        }

        ui.scope(|ui| {
            if reset_zoom {
                self.editor.reset_zoom(ui);
            }

            for response in self
                .editor
                .draw_graph_editor(ui, NoOp, sx, std::mem::take(&mut self.responses))
                .node_responses
            {
                match response {
                    NodeResponse::DisconnectEvent { output, input } => {
                        for (id, g) in &self.graph_items {
                            if let GraphItem::Link(o, i) = *g {
                                if output == o && input == i {
                                    sx.send(Request::DestroyObject(*id)).ok();
                                    break;
                                }
                            }
                        }

                        // Discard state change made by the user
                        self.editor.graph.add_connection(output, input, 0);
                    }
                    NodeResponse::ConnectEventEnded { output, input, .. } => {
                        let mut output_port = None;
                        let mut input_port = None;

                        for (id, object) in &self.graph_items {
                            match object {
                                GraphItem::InputPort(input_id) => {
                                    if input == *input_id {
                                        input_port = Some(*id);
                                        continue;
                                    }
                                }
                                GraphItem::OutputPort(output_id) => {
                                    if output == *output_id {
                                        output_port = Some(*id);
                                        continue;
                                    }
                                }
                                GraphItem::Link(o, i) => {
                                    if *o == output && *i == input {
                                        // Ports are already linked
                                        return;
                                    }
                                }
                                _ => {}
                            }
                        }

                        if let Some((output, input)) = output_port
                            .zip(input_port)
                            .map(|(output, input)| (output.to_string(), input.to_string()))
                        {
                            sx.send(Request::CreateObject(
                                ObjectType::Link,
                                String::from("link-factory"),
                                vec![
                                    ("link.output.port".to_owned(), output),
                                    ("link.input.port".to_owned(), input),
                                    ("object.linger".to_owned(), "true".to_owned()),
                                ],
                            ))
                            .ok();
                        }

                        // Discard state change made by the user
                        self.editor.graph.remove_connection(input, output);
                    }
                    _ => {}
                }
            }
        });
    }
}

#[cfg_attr(feature = "persistence", derive(serde::Serialize, serde::Deserialize))]
pub struct PersistentData {
    positions: HashMap<String, VecDeque<egui::Pos2>>,
    zoom: f32,
}

impl PersistentView for Graph {
    type Data = PersistentData;

    fn with_data(data: &Self::Data) -> Self {
        Self {
            restored_positions: Some(data.positions.clone()),

            editor: GraphEditorState::new(data.zoom),

            ..Self::new()
        }
    }

    fn save_data(&self) -> Option<Self::Data> {
        if self.editor.node_positions.is_empty() {
            // The graph hasn't been drawn, so nodes haven't been positioned
            return None;
        }

        let mut positions: HashMap<String, VecDeque<egui::Pos2>> = HashMap::new();

        for (&pos, node) in self.editor.graph.nodes.iter().filter_map(|(id, node)| {
            if let GraphNode::Node { global, .. } = &node.user_data {
                Some((self.editor.node_positions.get(id)?, global.upgrade()?))
            } else {
                None
            }
        }) {
            if let Some(name) = node.borrow().props().get("node.name") {
                positions
                    .entry(name.clone())
                    .and_modify(|e| e.push_back(pos))
                    .or_insert_with(|| vec![pos].into());
            }
        }

        Some(PersistentData {
            positions,
            zoom: self.editor.pan_zoom.zoom,
        })
    }
}
