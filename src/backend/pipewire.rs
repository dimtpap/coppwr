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

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    ffi::CString,
    ptr::NonNull,
    rc::Rc,
    sync::mpsc,
};

use pipewire as pw;
use pw::{
    permissions::Permissions,
    prelude::*,
    proxy::{Proxy, ProxyT},
    spa::pod::deserialize::PodDeserializer,
    types::ObjectType,
};

use crate::profiler_deserialize::{Profiling, Profilings};

pub enum ObjectMethod {
    ClientGetPermissions {
        index: u32,
        num: u32,
    },
    ClientUpdatePermissions(Vec<Permissions>),
    ClientUpdateProperties(BTreeMap<String, String>),
    MetadataSetProperty {
        subject: u32,
        key: String,
        type_: Option<String>,
        value: Option<String>,
    },
    MetadataClear,
}

pub enum PipeWireRequest {
    Stop,
    CreateObject(ObjectType, String, Vec<(String, String)>),
    DestroyObject(u32),
    LoadModule {
        module_dir: Option<String>,
        name: String,
        args: Option<String>,
        props: Option<Vec<(String, String)>>,
    },
    CallObjectMethod(u32, ObjectMethod),
}

pub enum PipeWireEvent {
    GlobalAdded(u32, ObjectType, Option<BTreeMap<String, String>>),
    GlobalRemoved(u32),
    GlobalInfo(u32, Box<[(&'static str, String)]>),
    GlobalProperties(u32, BTreeMap<String, String>),
    ClientPermissions(u32, u32, Vec<Permissions>),
    ProfilerProfile(Vec<Profiling>),
    MetadataProperty {
        id: u32,
        subject: u32,
        key: Option<String>,
        type_: Option<String>,
        value: Option<String>,
    },
}

pub fn run() -> (
    std::thread::JoinHandle<()>,
    mpsc::Receiver<PipeWireEvent>,
    pw::channel::Sender<PipeWireRequest>,
) {
    let (sx, rx) = mpsc::channel::<PipeWireEvent>();
    let (pwsx, pwrx) = pw::channel::channel::<PipeWireRequest>();

    (
        std::thread::spawn(move || pipewire_thread(sx, pwrx)),
        rx,
        pwsx,
    )
}

// Any object whose methods aren't used gets upcasted to a proxy
enum Global {
    Client(pw::client::Client),
    Metadata(pw::metadata::Metadata),
    Other(pw::proxy::Proxy),
}

impl Global {
    fn other(proxy: impl ProxyT) -> Self {
        Self::Other(proxy.upcast())
    }

    fn proxy(&self) -> &Proxy {
        match self {
            Self::Metadata(m) => m.upcast_ref(),
            Self::Client(c) => c.upcast_ref(),
            Self::Other(p) => p,
        }
    }
}

// Proxies created by core.create_object
struct LocalProxy(pw::proxy::Proxy, pw::proxy::ProxyListener);
struct BoundGlobal {
    global: Global,
    _object_listener: Box<dyn pw::proxy::Listener>,
    _proxy_listener: pw::proxy::ProxyListener,
}

fn dict_to_map(dict: &pw::spa::ForeignDict) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for (k, v) in dict.iter() {
        map.insert(k.to_string(), v.to_string());
    }
    map
}

fn key_val_to_props(
    kv: impl Iterator<Item = (impl Into<Vec<u8>>, impl Into<Vec<u8>>)>,
) -> pw::Properties {
    unsafe {
        let raw = NonNull::new(pw::sys::pw_properties_new(std::ptr::null())).unwrap();
        for (k, v) in kv
            .map(|(k, v)| (k.into(), v.into()))
            .filter(|(k, _)| !k.is_empty())
        {
            let k = CString::new(k).unwrap();
            let v = CString::new(v).unwrap();
            pw::sys::pw_properties_set(raw.as_ptr(), k.as_ptr(), v.as_ptr());
        }
        pw::Properties::from_ptr(raw)
    }
}

fn pipewire_thread(sx: mpsc::Sender<PipeWireEvent>, pwrx: pw::channel::Receiver<PipeWireRequest>) {
    let mainloop = pw::MainLoop::new().expect("Failed to create PipeWire mainloop");

    // Although the context is only moved in one callback
    // it must outlive the listener otherwise resource leaks occur.
    let context = Rc::new(pw::Context::new(&mainloop).expect("Failed to create PipeWire context"));

    if context
        .load_module("libpipewire-module-profiler", None, None)
        .is_err()
    {
        eprintln!("Failed to load the profiler module. No profiler data will be available");
    };

    let core = context
        .connect(Some(pw::properties! {
            "media.category" => "Manager" // Needed to get full permissions in Flatpak runtime
        }))
        .expect("Failed to connect to PipeWire remote");

    let registry = Rc::new(
        core.get_registry()
            .expect("Failed to get PipeWire registry"),
    );

    let binds = Rc::new(RefCell::new(HashMap::new()));

    let _receiver = pwrx.attach(&mainloop, {
        let mainloop = mainloop.clone();
        let context = Rc::clone(&context);
        let core = core.clone();
        let registry = Rc::clone(&registry);

        // Proxies created by core.create_object are kept seperate from proxies created
        // by registry binding because they've not been bound yet and need to be kept alive
        // until they become available in the registry and object listeners can be added on them
        let locals = Rc::new(RefCell::new(HashMap::new()));
        let binds = Rc::clone(&binds);

        move |msg| match msg {
            PipeWireRequest::Stop => {
                mainloop.quit();
            }
            PipeWireRequest::CreateObject(object_type, factory, props) => {
                let props = key_val_to_props(props.into_iter());

                let proxy = match object_type {
                    ObjectType::Link => core
                        .create_object::<pw::link::Link, _>(factory.as_str(), &props)
                        .map(ProxyT::upcast),
                    ObjectType::Port => core
                        .create_object::<pw::port::Port, _>(factory.as_str(), &props)
                        .map(ProxyT::upcast),
                    ObjectType::Node => core
                        .create_object::<pw::node::Node, _>(factory.as_str(), &props)
                        .map(ProxyT::upcast),
                    ObjectType::Client => core
                        .create_object::<pw::client::Client, _>(factory.as_str(), &props)
                        .map(ProxyT::upcast),
                    ObjectType::Device => core
                        .create_object::<pw::device::Device, _>(factory.as_str(), &props)
                        .map(ProxyT::upcast),
                    ObjectType::Factory => core
                        .create_object::<pw::factory::Factory, _>(factory.as_str(), &props)
                        .map(ProxyT::upcast),
                    ObjectType::Metadata => core
                        .create_object::<pw::metadata::Metadata, _>(factory.as_str(), &props)
                        .map(ProxyT::upcast),
                    ObjectType::Module => core
                        .create_object::<pw::module::Module, _>(factory.as_str(), &props)
                        .map(ProxyT::upcast),
                    ObjectType::Profiler => core
                        .create_object::<pw::profiler::Profiler, _>(factory.as_str(), &props)
                        .map(ProxyT::upcast),
                    _ => {
                        eprintln!("{object_type} unimplemented");
                        return;
                    }
                };

                match proxy {
                    Ok(proxy) => {
                        let id = proxy.id();
                        let listener = proxy
                            .add_listener_local()
                            .removed({
                                let locals = Rc::clone(&locals);
                                move || {
                                    locals.borrow_mut().remove(&id);
                                }
                            })
                            .error(move |_, res, msg| {
                                eprintln!("Local proxy {id} error: {res} - {msg}");
                            })
                            .register();

                        locals.borrow_mut().insert(id, LocalProxy(proxy, listener));
                    }
                    Err(e) => {
                        eprintln!("Error creating object from factory \"{factory}\" with properties {props:#?}: {e}");
                    }
                }
            }
            PipeWireRequest::DestroyObject(id) => {
                registry.destroy_global(id);
            }
            PipeWireRequest::LoadModule {
                module_dir,
                name,
                args,
                props,
            } => {
                let props = props.map(|props| key_val_to_props(props.into_iter()));

                let prev = std::env::var("PIPEWIRE_MODULE_DIR").ok();
                if let Some(ref module_dir) = module_dir {
                    std::env::set_var("PIPEWIRE_MODULE_DIR", module_dir);
                }

                if context
                    .load_module(name.as_str(), args.as_deref(), props)
                    .is_err()
                {
                    eprintln!("Failed to load module: Name: {name} - Directory: {module_dir:?} - Arguments: {args:?}");
                };

                if module_dir.is_some() {
                    if let Some(prev) = prev {
                        std::env::set_var("PIPEWIRE_MODULE_DIR", prev);
                    } else {
                        std::env::remove_var("PIPEWIRE_MODULE_DIR");
                    }
                }
            }
            PipeWireRequest::CallObjectMethod(id, method) => {
                let binds = binds.borrow();
                let Some(object) = binds.get(&id) else {
                    return;
                };

                match method {
                    ObjectMethod::ClientGetPermissions { index, num } => {
                        if let BoundGlobal {
                            global: Global::Client(client),
                            ..
                        } = object
                        {
                            client.get_permissions(index, num);
                        }
                    }
                    ObjectMethod::ClientUpdatePermissions(permissions) => {
                        if let BoundGlobal {
                            global: Global::Client(client),
                            ..
                        } = object
                        {
                            client.update_permissions(&permissions);
                        }
                    }
                    ObjectMethod::ClientUpdateProperties(props) => {
                        if let BoundGlobal {
                            global: Global::Client(client),
                            ..
                        } = object
                        {
                            client.update_properties(&key_val_to_props(props.into_iter()));
                        }
                    }
                    ObjectMethod::MetadataSetProperty {
                        subject,
                        key,
                        type_,
                        value,
                    } => {
                        if let BoundGlobal {
                            global: Global::Metadata(metadata),
                            ..
                        } = object
                        {
                            metadata.set_property(
                                subject,
                                key.as_str(),
                                type_.as_deref(),
                                value.as_deref(),
                            );
                        }
                    }
                    ObjectMethod::MetadataClear => {
                        if let BoundGlobal {
                            global: Global::Metadata(metadata),
                            ..
                        } = object
                        {
                            metadata.clear();
                        }
                    }
                }
            }
        }
    });

    sx.send(PipeWireEvent::GlobalAdded(0, ObjectType::Core, None))
        .ok();

    let _core_listener = core
        .add_listener_local()
        .info({
            let sx = sx.clone();
            move |info| {
                let infos = Box::new([
                    ("Name", info.name().to_string()),
                    ("Hostname", info.host_name().to_string()),
                    ("Username", info.user_name().to_string()),
                    ("Version", info.version().to_string()),
                    ("Cookie", info.cookie().to_string()),
                ]);

                sx.send(PipeWireEvent::GlobalInfo(0, infos)).ok();

                if let (true, Some(props)) = (
                    info.change_mask().contains(pw::ChangeMask::PROPS),
                    info.props(),
                ) {
                    sx.send(PipeWireEvent::GlobalProperties(0, dict_to_map(props)))
                        .ok();
                }
            }
        })
        .error(move |id, _, res, msg| eprintln!("Core: Error on proxy {id}: {res} - {msg}"))
        .register();

    let _registry_listener = registry
        .add_listener_local()
        .global({
            let sx = sx.clone();
            let registry = Rc::clone(&registry);
            let binds = Rc::clone(&binds);
            move |global| {
                if global.id == 0 {
                    return;
                }

                sx.send(PipeWireEvent::GlobalAdded(
                    global.id,
                    global.type_.clone(),
                    global.props.as_ref().map(dict_to_map),
                ))
                .ok();

                let id = global.id;
                let (bind, object_listener): (_, Box<dyn pw::proxy::Listener>) = match global.type_
                {
                    ObjectType::Module => {
                        if let Ok(module) = registry.bind::<pw::module::Module, _>(global) {
                            let listener = module
                                .add_listener_local()
                                .info({
                                    let sx = sx.clone();
                                    move |info| {
                                        let name = ("Name", info.name().to_string());
                                        let filename = ("Filename", info.filename().to_string());

                                        let infos: Box<[(&str, String)]> =
                                            if let Some(args) = info.args() {
                                                Box::new([
                                                    name,
                                                    filename,
                                                    ("Arguments", args.to_string()),
                                                ])
                                            } else {
                                                Box::new([name, filename])
                                            };

                                        sx.send(PipeWireEvent::GlobalInfo(id, infos)).ok();

                                        if let (true, Some(props)) = (
                                            info.change_mask()
                                                .contains(pw::module::ModuleChangeMask::PROPS),
                                            info.props(),
                                        ) {
                                            sx.send(PipeWireEvent::GlobalProperties(
                                                id,
                                                dict_to_map(props),
                                            ))
                                            .ok();
                                        }
                                    }
                                })
                                .register();
                            (Global::other(module), Box::new(listener))
                        } else {
                            eprintln!("Failed to bind to Module {id}");
                            return;
                        }
                    }
                    ObjectType::Factory => {
                        if let Ok(factory) = registry.bind::<pw::factory::Factory, _>(global) {
                            let listener = factory
                                .add_listener_local()
                                .info({
                                    let sx = sx.clone();
                                    move |info| {
                                        let infos = Box::new([
                                            ("Type", info.type_().to_string()),
                                            ("Version", info.version().to_string()),
                                        ]);

                                        sx.send(PipeWireEvent::GlobalInfo(id, infos)).ok();

                                        if let (true, Some(props)) = (
                                            info.change_mask()
                                                .contains(pw::factory::FactoryChangeMask::PROPS),
                                            info.props(),
                                        ) {
                                            sx.send(PipeWireEvent::GlobalProperties(
                                                id,
                                                dict_to_map(props),
                                            ))
                                            .ok();
                                        }
                                    }
                                })
                                .register();
                            (Global::other(factory), Box::new(listener))
                        } else {
                            eprintln!("Failed to bind to Factory {id}");
                            return;
                        }
                    }
                    ObjectType::Device => {
                        if let Ok(device) = registry.bind::<pw::device::Device, _>(global) {
                            let listener = device
                                .add_listener_local()
                                .info({
                                    let sx = sx.clone();
                                    move |info| {
                                        if let (true, Some(props)) = (
                                            info.change_mask()
                                                .contains(pw::device::DeviceChangeMask::PROPS),
                                            info.props(),
                                        ) {
                                            sx.send(PipeWireEvent::GlobalProperties(
                                                id,
                                                dict_to_map(props),
                                            ))
                                            .ok();
                                        }
                                    }
                                })
                                .register();
                            (Global::other(device), Box::new(listener))
                        } else {
                            eprintln!("Failed to bind to Device {id}");
                            return;
                        }
                    }
                    ObjectType::Client => {
                        if let Ok(client) = registry.bind::<pw::client::Client, _>(global) {
                            let listener = client
                                .add_listener_local()
                                .info({
                                    let sx = sx.clone();
                                    move |info| {
                                        if let (true, Some(props)) = (
                                            info.change_mask()
                                                .contains(pw::client::ClientChangeMask::PROPS),
                                            info.props(),
                                        ) {
                                            sx.send(PipeWireEvent::GlobalProperties(
                                                id,
                                                dict_to_map(props),
                                            ))
                                            .ok();
                                        }
                                    }
                                })
                                .permissions({
                                    let sx = sx.clone();
                                    move |idx, permissions| {
                                        sx.send(PipeWireEvent::ClientPermissions(
                                            id,
                                            idx,
                                            permissions.into(),
                                        ))
                                        .ok();
                                    }
                                })
                                .register();
                            (Global::Client(client), Box::new(listener))
                        } else {
                            eprintln!("Failed to bind to Client {id}");
                            return;
                        }
                    }
                    ObjectType::Node => {
                        if let Ok(node) = registry.bind::<pw::node::Node, _>(global) {
                            let listener = node
                                .add_listener_local()
                                .info({
                                    let sx = sx.clone();
                                    move |info| {
                                        let state = match info.state() {
                                            pw::node::NodeState::Creating => "Creating",
                                            pw::node::NodeState::Idle => "Idle",
                                            pw::node::NodeState::Suspended => "Suspended",
                                            pw::node::NodeState::Running => "Running",
                                            pw::node::NodeState::Error(e) => e,
                                        }
                                        .to_string();
                                        let infos = Box::new([
                                            ("Max Input Ports", info.max_input_ports().to_string()),
                                            (
                                                "Max Output Ports",
                                                info.max_output_ports().to_string(),
                                            ),
                                            ("Input Ports", info.n_input_ports().to_string()),
                                            ("Output Ports", info.n_output_ports().to_string()),
                                            ("State", state),
                                        ]);

                                        sx.send(PipeWireEvent::GlobalInfo(id, infos)).ok();

                                        if let (true, Some(props)) = (
                                            info.change_mask()
                                                .contains(pw::node::NodeChangeMask::PROPS),
                                            info.props(),
                                        ) {
                                            sx.send(PipeWireEvent::GlobalProperties(
                                                id,
                                                dict_to_map(props),
                                            ))
                                            .ok();
                                        }
                                    }
                                })
                                .register();
                            (Global::other(node), Box::new(listener))
                        } else {
                            eprintln!("Failed to bind to Node {id}");
                            return;
                        }
                    }
                    ObjectType::Port => {
                        if let Ok(port) = registry.bind::<pw::port::Port, _>(global) {
                            let listener = port
                                .add_listener_local()
                                .info({
                                    let sx = sx.clone();
                                    move |info| {
                                        let direction = match info.direction() {
                                            pw::spa::Direction::Input => "Input",
                                            pw::spa::Direction::Output => "Output",
                                        }
                                        .to_string();

                                        sx.send(PipeWireEvent::GlobalInfo(
                                            id,
                                            Box::new([("Direction", direction)]),
                                        ))
                                        .ok();

                                        if let (true, Some(props)) = (
                                            info.change_mask()
                                                .contains(pw::port::PortChangeMask::PROPS),
                                            info.props(),
                                        ) {
                                            sx.send(PipeWireEvent::GlobalProperties(
                                                id,
                                                dict_to_map(props),
                                            ))
                                            .ok();
                                        }
                                    }
                                })
                                .register();
                            (Global::other(port), Box::new(listener))
                        } else {
                            eprintln!("Failed to bind to Port {id}");
                            return;
                        }
                    }
                    ObjectType::Link => {
                        if let Ok(link) = registry.bind::<pw::link::Link, _>(global) {
                            let listener = link
                                .add_listener_local()
                                .info({
                                    let sx = sx.clone();
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
                                        .to_string();
                                        let infos = Box::new([
                                            ("Input Node ID", info.input_node_id().to_string()),
                                            ("Intput Port ID", info.input_port_id().to_string()),
                                            ("Output Node ID", info.output_node_id().to_string()),
                                            ("Output Port ID", info.output_port_id().to_string()),
                                            ("State", state),
                                        ]);

                                        sx.send(PipeWireEvent::GlobalInfo(id, infos)).ok();

                                        if let (true, Some(props)) = (
                                            info.change_mask()
                                                .contains(pw::link::LinkChangeMask::PROPS),
                                            info.props(),
                                        ) {
                                            sx.send(PipeWireEvent::GlobalProperties(
                                                id,
                                                dict_to_map(props),
                                            ))
                                            .ok();
                                        }
                                    }
                                })
                                .register();
                            (Global::other(link), Box::new(listener))
                        } else {
                            eprintln!("Failed to bind to Link {id}");
                            return;
                        }
                    }
                    ObjectType::Profiler => {
                        if let Ok(profiler) = registry.bind::<pw::profiler::Profiler, _>(global) {
                            let listener = profiler
                                .add_listener_local()
                                .profile({
                                    let sx = sx.clone();
                                    move |pod| {
                                        if let Some(pod) = NonNull::new(pod.cast_mut()) {
                                            match unsafe {
                                                PodDeserializer::deserialize_ptr::<Profilings>(pod)
                                            } {
                                                Ok(profilings) => {
                                                    sx.send(PipeWireEvent::ProfilerProfile(
                                                        profilings.0,
                                                    ))
                                                    .ok();
                                                }
                                                Err(_) => {
                                                    eprintln!(
                                                        "Deserialization of profiler {id} statistics failed"
                                                    );
                                                }
                                            }
                                        }
                                    }
                                })
                                .register();
                            (Global::other(profiler), Box::new(listener))
                        } else {
                            eprintln!("Failed to bind to Profiler {id}");
                            return;
                        }
                    }
                    ObjectType::Metadata => {
                        if let Ok(metadata) = registry.bind::<pw::metadata::Metadata, _>(global) {
                            let listener = metadata
                            .add_listener_local()
                            .property({
                                let sx = sx.clone();
                                move |subject, key, type_, value| {
                                    sx.send(PipeWireEvent::MetadataProperty {
                                        id,
                                        subject,
                                        key: key.map(str::to_string),
                                        type_: type_.map(str::to_string),
                                        value: value.map(str::to_string),
                                    })
                                    .ok();
                                    0
                                }
                            })
                            .register();
                            (Global::Metadata(metadata), Box::new(listener))
                        } else {
                            eprintln!("Failed to bind to Metadata {id}");
                            return;
                        }
                    }
                    _ => {
                        return;
                    }
                };

                let proxy_listener = bind
                    .proxy()
                    .add_listener_local()
                    .removed({
                        let binds = Rc::clone(&binds);
                        move || {
                            binds.borrow_mut().remove(&id);
                        }
                    })
                    .register();

                binds.borrow_mut().insert(
                    id,
                    BoundGlobal {
                        global: bind,
                        _object_listener: object_listener,
                        _proxy_listener: proxy_listener,
                    },
                );
            }
        })
        .global_remove({
            let sx = sx.clone();
            move |id| {
                sx.send(PipeWireEvent::GlobalRemoved(id)).ok();
            }
        })
        .register();

    mainloop.run();
}
