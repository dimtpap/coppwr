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

#[path = "listeners.rs"]
mod listeners;

use pipewire as pw;
use pw::{
    proxy::{Proxy, ProxyT},
    registry::GlobalObject,
    spa::ForeignDict,
    types::ObjectType,
};

use super::{util, Event, ObjectMethod};

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

    pub fn proxy(&self) -> &Proxy {
        match self {
            Self::Metadata(m) => m.upcast_ref(),
            Self::Client(c) => c.upcast_ref(),
            Self::Other(p) => p,
        }
    }

    pub fn as_client(&self) -> Option<&pw::client::Client> {
        if let Global::Client(client) = self {
            Some(client)
        } else {
            None
        }
    }

    pub fn as_metadata(&self) -> Option<&pw::metadata::Metadata> {
        if let Global::Metadata(metadata) = self {
            Some(metadata)
        } else {
            None
        }
    }
}

pub struct BoundGlobal {
    global: Global,
    _object_listener: Box<dyn pw::proxy::Listener>,
    _proxy_listener: pw::proxy::ProxyListener,
}

pub enum BindError {
    Unimplemented,
    PipeWireError(pw::Error),
}

impl From<pw::Error> for BindError {
    fn from(value: pw::Error) -> Self {
        Self::PipeWireError(value)
    }
}

impl BoundGlobal {
    pub fn bind_to(
        registry: &pw::registry::Registry,
        global: &GlobalObject<ForeignDict>,
        sx: &std::sync::mpsc::Sender<Event>,
        proxy_removed: impl Fn() + 'static,
    ) -> Result<Self, BindError> {
        let id = global.id;
        let (global, _object_listener): (_, Box<dyn pw::proxy::Listener>) = match global.type_ {
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
                return Err(BindError::Unimplemented);
            }
        };

        let _proxy_listener = global
            .proxy()
            .add_listener_local()
            .removed(proxy_removed)
            .register();

        Ok(Self {
            global,
            _object_listener,
            _proxy_listener,
        })
    }

    pub fn call(&self, method: ObjectMethod) {
        match method {
            ObjectMethod::ClientGetPermissions { index, num } => {
                if let Some(client) = self.global.as_client() {
                    client.get_permissions(index, num);
                }
            }
            ObjectMethod::ClientUpdatePermissions(permissions) => {
                if let Some(client) = self.global.as_client() {
                    client.update_permissions(&permissions);
                }
            }
            ObjectMethod::ClientUpdateProperties(props) => {
                if let Some(client) = self.global.as_client() {
                    client.update_properties(&util::key_val_to_props(props.into_iter()));
                }
            }
            ObjectMethod::MetadataSetProperty {
                subject,
                key,
                type_,
                value,
            } => {
                if let Some(metadata) = self.global.as_metadata() {
                    metadata.set_property(
                        subject,
                        key.as_str(),
                        type_.as_deref(),
                        value.as_deref(),
                    );
                }
            }
            ObjectMethod::MetadataClear => {
                if let Some(metadata) = self.global.as_metadata() {
                    metadata.clear();
                }
            }
        }
    }
}
