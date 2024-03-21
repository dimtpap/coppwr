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

#[path = "listeners.rs"]
mod listeners;

use pipewire::{
    self as pw,
    proxy::{Proxy, ProxyT},
    registry::GlobalObject,
    spa::utils::dict::DictRef,
    types::ObjectType,
};

use super::{util, Event, ObjectMethod};

#[derive(Debug)]
pub enum Error {
    Unimplemented(ObjectType),
    PipeWire(pw::Error),
}

impl From<pw::Error> for Error {
    fn from(value: pw::Error) -> Self {
        Self::PipeWire(value)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unimplemented(object_type) => f.write_fmt(format_args!(
                "Unsupported PipeWire object type {object_type}"
            )),
            Self::PipeWire(e) => f.write_fmt(format_args!("PipeWire error: {e}")),
        }
    }
}

impl std::error::Error for Error {}

// Objects whose methods aren't used get upcasted to a proxy
pub enum Global {
    Client(pw::client::Client),
    Metadata(pw::metadata::Metadata),
    Other(pw::proxy::Proxy),
}

impl Global {
    pub fn other(proxy: impl ProxyT) -> Self {
        Self::Other(proxy.upcast())
    }

    pub fn as_proxy(&self) -> &Proxy {
        match self {
            Self::Metadata(m) => m.upcast_ref(),
            Self::Client(c) => c.upcast_ref(),
            Self::Other(p) => p,
        }
    }
}

pub struct BoundGlobal {
    global: Global,
    _object_listener: Box<dyn pw::proxy::Listener>,
    _proxy_listener: pw::proxy::ProxyListener,
}

impl BoundGlobal {
    pub fn bind_to<P: AsRef<DictRef>>(
        registry: &pw::registry::Registry,
        global: &GlobalObject<&P>,
        sx: &std::sync::mpsc::Sender<Event>,
        proxy_removed: impl Fn() + 'static,
    ) -> Result<Self, Error> {
        let sx = sx.clone();

        let id = global.id;
        let (global, object_listener): (_, Box<dyn pw::proxy::Listener>) = match global.type_ {
            ObjectType::Module => {
                listeners::module(registry.bind::<pw::module::Module, _>(global)?, id, sx)
            }
            ObjectType::Factory => {
                listeners::factory(registry.bind::<pw::factory::Factory, _>(global)?, id, sx)
            }
            ObjectType::Device => {
                listeners::device(registry.bind::<pw::device::Device, _>(global)?, id, sx)
            }
            ObjectType::Client => {
                listeners::client(registry.bind::<pw::client::Client, _>(global)?, id, sx)
            }
            ObjectType::Node => {
                listeners::node(registry.bind::<pw::node::Node, _>(global)?, id, sx)
            }
            ObjectType::Port => {
                listeners::port(registry.bind::<pw::port::Port, _>(global)?, id, sx)
            }
            ObjectType::Link => {
                listeners::link(registry.bind::<pw::link::Link, _>(global)?, id, sx)
            }
            ObjectType::Profiler => {
                listeners::profiler(registry.bind::<pw::profiler::Profiler, _>(global)?, id, sx)
            }
            ObjectType::Metadata => {
                listeners::metadata(registry.bind::<pw::metadata::Metadata, _>(global)?, id, sx)
            }
            _ => {
                return Err(Error::Unimplemented(global.type_.clone()));
            }
        };

        let proxy_listener = global
            .as_proxy()
            .add_listener_local()
            .removed(proxy_removed)
            .register();

        Ok(Self {
            global,
            _object_listener: object_listener,
            _proxy_listener: proxy_listener,
        })
    }

    pub fn call(&self, method: ObjectMethod) {
        match method {
            ObjectMethod::ClientGetPermissions { index, num } => {
                if let Global::Client(ref client) = self.global {
                    client.get_permissions(index, num);
                }
            }
            ObjectMethod::ClientUpdatePermissions(permissions) => {
                if let Global::Client(ref client) = self.global {
                    client.update_permissions(&permissions);
                }
            }
            ObjectMethod::ClientUpdateProperties(props) => {
                if let Global::Client(ref client) = self.global {
                    client.update_properties(util::key_val_to_props(props.into_iter()).dict());
                }
            }
            ObjectMethod::MetadataSetProperty {
                subject,
                key,
                type_,
                value,
            } => {
                if let Global::Metadata(ref metadata) = self.global {
                    metadata.set_property(
                        subject,
                        key.as_str(),
                        type_.as_deref(),
                        value.as_deref(),
                    );
                }
            }
            ObjectMethod::MetadataClear => {
                if let Global::Metadata(ref metadata) = self.global {
                    metadata.clear();
                }
            }
        }
    }
}
