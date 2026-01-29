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

use pipewire::{
    self as pw,
    spa::{param::ParamType, pod::deserialize::PodDeserializer},
};

use crate::backend::{Event, bind::Global, pods::profiler, util::dict_to_map};

type Bind = (Global, Box<dyn pipewire::proxy::Listener>);

pub fn module(module: pw::module::Module, id: u32, on_event: impl Fn(Event) + 'static) -> Bind {
    let listener = module
        .add_listener_local()
        .info({
            move |info| {
                let name = ("Name", info.name().to_owned());
                let filename = ("Filename", info.filename().to_owned());

                let infos: Box<[(&str, String)]> = if let Some(args) = info.args() {
                    Box::new([name, filename, ("Arguments", args.to_owned())])
                } else {
                    Box::new([name, filename])
                };

                let props = info
                    .change_mask()
                    .contains(pw::module::ModuleChangeMask::PROPS)
                    .then_some(info.props())
                    .flatten()
                    .map(dict_to_map);

                on_event(Event::GlobalInfo(id, infos, props));
            }
        })
        .register();
    (Global::other(module), Box::new(listener))
}

pub fn factory(factory: pw::factory::Factory, id: u32, on_event: impl Fn(Event) + 'static) -> Bind {
    let listener = factory
        .add_listener_local()
        .info({
            move |info| {
                let infos = Box::new([
                    ("Type", info.type_().to_string()),
                    ("Version", info.version().to_string()),
                ]);

                let props = info
                    .change_mask()
                    .contains(pw::factory::FactoryChangeMask::PROPS)
                    .then_some(info.props())
                    .flatten()
                    .map(dict_to_map);

                on_event(Event::GlobalInfo(id, infos, props));
            }
        })
        .register();
    (Global::other(factory), Box::new(listener))
}

pub fn device(device: pw::device::Device, id: u32, on_event: impl Fn(Event) + 'static) -> Bind {
    let listener = device
        .add_listener_local()
        .info({
            move |info| {
                let props = info
                    .change_mask()
                    .contains(pw::device::DeviceChangeMask::PROPS)
                    .then_some(info.props())
                    .flatten()
                    .map(dict_to_map);

                on_event(Event::GlobalInfo(id, Box::new([]), props));
            }
        })
        .register();
    (Global::other(device), Box::new(listener))
}

pub fn client(
    client: pw::client::Client,
    id: u32,
    on_event: impl Fn(Event) + Clone + 'static,
) -> Bind {
    let listener = client
        .add_listener_local()
        .info({
            let on_event = on_event.clone();
            move |info| {
                let props = info
                    .change_mask()
                    .contains(pw::client::ClientChangeMask::PROPS)
                    .then_some(info.props())
                    .flatten()
                    .map(dict_to_map);

                on_event(Event::GlobalInfo(id, Box::new([]), props));
            }
        })
        .permissions({
            move |idx, permissions| {
                on_event(Event::ClientPermissions(id, idx, permissions.into()));
            }
        })
        .register();
    (Global::Client(client), Box::new(listener))
}

pub fn node(node: pw::node::Node, id: u32, on_event: impl Fn(Event) + 'static) -> Bind {
    let listener = node
        .add_listener_local()
        .info({
            move |info| {
                let state = match info.state() {
                    pw::node::NodeState::Creating => "Creating",
                    pw::node::NodeState::Idle => "Idle",
                    pw::node::NodeState::Suspended => "Suspended",
                    pw::node::NodeState::Running => "Running",
                    pw::node::NodeState::Error(e) => e,
                }
                .to_owned();
                let infos = Box::new([
                    ("Max Input Ports", info.max_input_ports().to_string()),
                    ("Max Output Ports", info.max_output_ports().to_string()),
                    ("Input Ports", info.n_input_ports().to_string()),
                    ("Output Ports", info.n_output_ports().to_string()),
                    ("State", state),
                ]);

                let props = info
                    .change_mask()
                    .contains(pw::node::NodeChangeMask::PROPS)
                    .then_some(info.props())
                    .flatten()
                    .map(dict_to_map);

                on_event(Event::GlobalInfo(id, infos, props));
            }
        })
        .register();
    (Global::other(node), Box::new(listener))
}

pub fn port(port: pw::port::Port, id: u32, on_event: impl Fn(Event) + Clone + 'static) -> Bind {
    port.subscribe_params(&[ParamType::EnumFormat]);

    let listener = port
        .add_listener_local()
        .info({
            let on_event = on_event.clone();
            move |info| {
                let direction = match info.direction() {
                    pw::spa::utils::Direction::Input => "Input",
                    pw::spa::utils::Direction::Output => "Output",
                    _ => "Invalid",
                }
                .to_owned();

                let props = info
                    .change_mask()
                    .contains(pw::port::PortChangeMask::PROPS)
                    .then_some(info.props())
                    .flatten()
                    .map(dict_to_map);

                on_event(Event::GlobalInfo(
                    id,
                    Box::new([("Direction", direction)]),
                    props,
                ));
            }
        })
        .param(move |_, _, _, _, pod| {
            let Some(pod) = pod else {
                return;
            };

            match pw::spa::param::format_utils::parse_format(pod) {
                Ok((media_type, _)) => {
                    on_event(Event::PortMediaType { id, media_type });
                }
                Err(e) => {
                    println!("Failed to parse port param: {e}");
                }
            }
        })
        .register();
    (Global::other(port), Box::new(listener))
}

pub fn link(link: pw::link::Link, id: u32, on_event: impl Fn(Event) + 'static) -> Bind {
    let listener = link
        .add_listener_local()
        .info({
            move |info| {
                let state = match info.state() {
                    pw::link::LinkState::Init => "Init",
                    pw::link::LinkState::Allocating => "Allocating",
                    pw::link::LinkState::Negotiating => "Negotiating",
                    pw::link::LinkState::Active => "Active",
                    pw::link::LinkState::Paused => "Paused",
                    pw::link::LinkState::Unlinked => "Unlinked",
                    pw::link::LinkState::Error(e) => e,
                }
                .to_owned();
                let infos = Box::new([
                    ("Input Node ID", info.input_node_id().to_string()),
                    ("Input Port ID", info.input_port_id().to_string()),
                    ("Output Node ID", info.output_node_id().to_string()),
                    ("Output Port ID", info.output_port_id().to_string()),
                    ("State", state),
                ]);

                let props = info
                    .change_mask()
                    .contains(pw::link::LinkChangeMask::PROPS)
                    .then_some(info.props())
                    .flatten()
                    .map(dict_to_map);

                on_event(Event::GlobalInfo(id, infos, props));
            }
        })
        .register();
    (Global::other(link), Box::new(listener))
}

pub fn profiler(
    profiler: pw::profiler::Profiler,
    id: u32,
    on_event: impl Fn(Event) + 'static,
) -> Bind {
    let listener = profiler
        .add_listener_local()
        .profile({
            move |pod| match PodDeserializer::deserialize_from::<profiler::Profilings>(pod) {
                Ok((_, profilings)) => {
                    on_event(Event::ProfilerProfile(profilings.0));
                }
                Err(e) => {
                    eprintln!("Deserialization of profiler {id} statistics failed: {e:?}");
                }
            }
        })
        .register();
    (Global::other(profiler), Box::new(listener))
}

pub fn metadata(
    metadata: pw::metadata::Metadata,
    id: u32,
    on_event: impl Fn(Event) + 'static,
) -> Bind {
    let listener = metadata
        .add_listener_local()
        .property({
            move |subject, key, type_, value| {
                on_event(Event::MetadataProperty {
                    id,
                    subject,
                    key: key.map(ToOwned::to_owned),
                    type_: type_.map(ToOwned::to_owned),
                    value: value.map(ToOwned::to_owned),
                });
                0
            }
        })
        .register();
    (Global::Metadata(metadata), Box::new(listener))
}
