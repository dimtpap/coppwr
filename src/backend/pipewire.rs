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
    rc::Rc,
    sync::mpsc,
};

use pipewire as pw;
use pw::{permissions::Permissions, proxy::ProxyT, types::ObjectType};

use super::{
    bind::{BindError, BoundGlobal, Global},
    profiler::Profiling,
    util,
};

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

pub enum Request {
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

pub enum Event {
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
    Stop,
}

pub fn run(
    remote: String,
) -> (
    std::thread::JoinHandle<()>,
    mpsc::Receiver<Event>,
    pw::channel::Sender<Request>,
) {
    let (sx, rx) = mpsc::channel::<Event>();
    let (pwsx, pwrx) = pw::channel::channel::<Request>();

    (
        std::thread::spawn(move || pipewire_thread(remote.as_str(), sx, pwrx)),
        rx,
        pwsx,
    )
}

// Proxies created by core.create_object
struct LocalProxy(pw::proxy::Proxy, pw::proxy::ProxyListener);

fn pipewire_thread(remote: &str, sx: mpsc::Sender<Event>, pwrx: pw::channel::Receiver<Request>) {
    let Ok((mainloop, context, core, registry))
        : Result<(pw::MainLoop, Rc<pw::Context<pw::MainLoop>>, pw::Core, Rc<pw::registry::Registry>), pw::Error> = (|| {
        let mainloop = pw::MainLoop::new()?;

        let context = pw::Context::new(&mainloop)?;
        if context
            .load_module("libpipewire-module-profiler", None, None)
            .is_err()
        {
            eprintln!("Failed to load the profiler module. No profiler data will be available");
        };

        let env_remote = std::env::var("PIPEWIRE_REMOTE").ok();
        std::env::remove_var("PIPEWIRE_REMOTE");

        let core = context.connect(Some(util::key_val_to_props(
            [("media.category", "Manager"), ("remote.name", remote)].into_iter(),
        )))?;

        if let Some(env_remote) = env_remote {
            std::env::set_var("PIPEWIRE_REMOTE", env_remote);
        }

        let registry = core.get_registry()?;

        // Context needs to be moved to the loop listener
        // but must outlive it to prevent resource leaks
        Ok((mainloop, Rc::new(context), core, Rc::new(registry)))
    })() else {
        sx.send(Event::Stop).ok();
        return;
    };

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
            Request::Stop => {
                mainloop.quit();
            }
            Request::CreateObject(object_type, factory, props) => {
                let props = util::key_val_to_props(props.into_iter());

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
            Request::DestroyObject(id) => {
                registry.destroy_global(id);
            }
            Request::LoadModule {
                module_dir,
                name,
                args,
                props,
            } => {
                let props = props.map(|props| util::key_val_to_props(props.into_iter()));

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
            Request::CallObjectMethod(id, method) => {
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
                            client.update_properties(&util::key_val_to_props(props.into_iter()));
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

    sx.send(Event::GlobalAdded(0, ObjectType::Core, None)).ok();

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

                sx.send(Event::GlobalInfo(0, infos)).ok();

                if let (true, Some(props)) = (
                    info.change_mask().contains(pw::ChangeMask::PROPS),
                    info.props(),
                ) {
                    sx.send(Event::GlobalProperties(0, util::dict_to_map(props)))
                        .ok();
                }
            }
        })
        .error({
            let sx = sx.clone();
            let mainloop = mainloop.clone();
            move |id, _, res, msg| {
                eprintln!("Core: Error on proxy {id}: {res} - {msg}");

                // -EPIPE on the core proxy usually means the remote has been closed
                if id == 0 && res == -32 {
                    mainloop.quit();
                    sx.send(Event::Stop).ok();
                }
            }
        })
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

                sx.send(Event::GlobalAdded(
                    global.id,
                    global.type_.clone(),
                    global.props.as_ref().map(util::dict_to_map),
                ))
                .ok();

                let id = global.id;
                match BoundGlobal::bind_to(&registry, global, &sx, {
                    let binds = binds.clone();
                    move || {
                        binds.borrow_mut().remove(&id);
                    }
                }) {
                    Ok(bound_global) => {
                        binds.borrow_mut().insert(id, bound_global);
                    }
                    Err(e) => match e {
                        BindError::Unimplemented => {
                            eprintln!("Unsupported object type {}", global.type_);
                        }
                        BindError::PipeWireError(e) => {
                            eprintln!("Error binding object {id}: {e}");
                        }
                    },
                }
            }
        })
        .global_remove({
            let sx = sx.clone();
            move |id| {
                sx.send(Event::GlobalRemoved(id)).ok();
            }
        })
        .register();

    mainloop.run();

    sx.send(Event::Stop).ok();
}
