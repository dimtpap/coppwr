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
    borrow::Cow,
    cell::RefCell,
    collections::{HashMap, VecDeque},
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
    ui::{globals_store::Global, util::persistence::PersistentView},
};

// Used to satisfy trait bounds that provide unneded features
#[derive(Debug, Default, Clone)]
struct NoOp;
impl egui_node_graph::WidgetValueTrait for NoOp {
    type Response = Self;
    type NodeData = Node;
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
    type NodeData = Node;
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
        unreachable!("The node finder/creator should never be shown")
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

struct Node {
    media_type: MediaType,
    global: Weak<RefCell<Global>>,
}

impl Node {
    const fn new(media_type: MediaType, global: Weak<RefCell<Global>>) -> Self {
        Self { media_type, global }
    }
}

impl NodeDataTrait for Node {
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
        if let Some(global) = self.global.upgrade() {
            egui::CollapsingHeader::new("Details")
                .default_open(true)
                .show_unindented(ui, |ui| {
                    egui::Frame::central_panel(&egui::Style::default())
                        .inner_margin(egui::Margin::same(3))
                        .corner_radius(ui.visuals().noninteractive().corner_radius)
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

    editor: egui_node_graph::GraphEditorState<Node, MediaType, NoOp, NoOp, backend::Sender>,
    responses: Vec<NodeResponse<NoOp, Node>>,

    // Maps PipeWire global IDs to graph items
    items: HashMap<u32, GraphItem>,
}

impl Graph {
    pub fn new() -> Self {
        Self {
            restored_positions: None,

            editor: GraphEditorState::default(),
            responses: Vec::new(),
            items: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, id: u32, global: &Rc<RefCell<Global>>) {
        if self.items.contains_key(&id) {
            return;
        }

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
            Node::new(media_type, Rc::downgrade(global)),
            |_, _| {},
        );

        self.responses.push(NodeResponse::CreatedNode(graph_id));

        self.items.insert(id, graph_id.into());
    }

    fn port_graph_node_and_media_type(
        &self,
        id: u32,
        node_id: u32,
    ) -> Option<(&NodeId, MediaType)> {
        if self.items.contains_key(&id) {
            return None;
        }

        let Some(GraphItem::Node(node_id)) = self.items.get(&node_id) else {
            return None;
        };

        Some((
            node_id,
            self.editor
                .graph
                .nodes
                .get(*node_id)
                .unwrap()
                .user_data
                .media_type,
        ))
    }

    pub fn add_input_port(
        &mut self,
        id: u32,
        node_id: u32,
        name: String,
        media_type: Option<MediaType>,
    ) {
        let Some((node_id, parent_media_type)) = self.port_graph_node_and_media_type(id, node_id)
        else {
            return;
        };

        let media_type = media_type.unwrap_or(parent_media_type);

        let graph_id = self.editor.graph.add_wide_input_param(
            *node_id,
            name,
            media_type,
            NoOp,
            egui_node_graph::InputParamKind::ConnectionOnly,
            None,
            true,
        );

        self.items.insert(id, graph_id.into());
    }

    pub fn add_output_port(
        &mut self,
        id: u32,
        node_id: u32,
        name: String,
        media_type: Option<MediaType>,
    ) {
        let Some((node_id, parent_media_type)) = self.port_graph_node_and_media_type(id, node_id)
        else {
            return;
        };

        let media_type = media_type.unwrap_or(parent_media_type);

        let graph_id = self
            .editor
            .graph
            .add_output_param(*node_id, name, media_type);

        self.items.insert(id, graph_id.into());
    }

    pub fn add_link(&mut self, id: u32, output_port_id: u32, input_port_id: u32) {
        if self.items.contains_key(&id) {
            return;
        }

        let Some((GraphItem::OutputPort(output), GraphItem::InputPort(input))) = self
            .items
            .get(&output_port_id)
            .zip(self.items.get(&input_port_id))
        else {
            return;
        };

        self.editor.graph.add_connection(*output, *input, 0);

        self.items.insert(id, GraphItem::Link(*output, *input));
    }

    pub fn remove_item(&mut self, id: u32) {
        let Some(item) = self.items.remove(&id) else {
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

        let reset_view = ui
            .horizontal(|ui| {
                if ui.button("Auto arrange").clicked() {
                    self.editor.node_positions.clear();
                    self.editor.node_order.clear();
                    self.editor.pan_zoom.pan = egui::Vec2::ZERO;
                }

                ui.label("Zoom");
                ui.add(
                    egui::Slider::new(&mut self.editor.pan_zoom.zoom, 0.2..=2.0).max_decimals(2),
                );

                ui.button("Reset view").clicked()
            })
            .inner;
        ui.separator();

        const NODE_SPACING: egui::Vec2 = egui::vec2(200f32, 100f32);

        let mut next_outputs_only_pos = egui::Pos2::ZERO;
        let mut next_default_pos =
            egui::Pos2::new((ui.available_width() - NODE_SPACING.x) / 2., 0f32);
        let mut next_inputs_only_pos = egui::Pos2::new(
            ui.available_width()
                - NODE_SPACING.x
                - f32::from(ui.style().spacing.window_margin.right),
            0f32,
        );

        for pos in self.editor.node_positions.values_mut() {
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

            self.editor.node_order.push(id);

            let mut ports = None;

            if let Some(global) = node.user_data.global.upgrade() {
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
            }

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
            ui.shrink_clip_rect(ui.max_rect().expand(ui.visuals().clip_rect_margin));

            if reset_view {
                self.editor.reset_zoom(ui);
                self.editor.pan_zoom.pan = egui::Vec2::ZERO;
            }

            for response in self
                .editor
                .draw_graph_editor(ui, NoOp, sx, std::mem::take(&mut self.responses))
                .node_responses
            {
                match response {
                    NodeResponse::DisconnectEvent { output, input } => {
                        for (id, g) in &self.items {
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

                        for (id, object) in &self.items {
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

            // Can only be queried after the editor UI has been drawn
            if ui.ui_contains_pointer() {
                let (secondary_down, pointer_delta) =
                    ui.input(|i| (i.pointer.secondary_down(), i.pointer.delta()));

                if secondary_down {
                    self.editor.pan_zoom.pan += pointer_delta;
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
            Some((
                self.editor.node_positions.get(id)?,
                node.user_data.global.upgrade()?,
            ))
        }) {
            if let Some(name) = node.borrow().props().get("node.name") {
                positions.entry(name.clone()).or_default().push_back(pos);
            }
        }

        Some(PersistentData {
            positions,
            zoom: self.editor.pan_zoom.zoom,
        })
    }
}
