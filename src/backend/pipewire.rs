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

use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::mpsc};

use super::{
    bind::{BoundGlobal, Error},
    pw::{self, proxy::ProxyT, types::ObjectType},
    util, Connection, Event, RemoteInfo, Request,
};

#[cfg(feature = "pw_v0_3_77")]
use super::REMOTE_VERSION;

pub fn pipewire_thread(
    remote: RemoteInfo,
    context_properties: Vec<(String, String)>,
    sx: mpsc::Sender<Event>,
    pwrx: pw::channel::Receiver<Request>,
) {
    // Proxies created by core.create_object
    struct LocalProxy(pw::proxy::Proxy, pw::proxy::ProxyListener);

    let (mainloop, context, connection, registry): (
        pw::MainLoop,
        Rc<pw::Context<pw::MainLoop>>,
        Connection,
        Rc<pw::registry::Registry>,
    ) = match (|| {
        let mainloop = pw::MainLoop::new()?;

        let context = pw::Context::new(&mainloop)?;
        if context
            .load_module("libpipewire-module-profiler", None, None)
            .is_err()
        {
            eprintln!("Failed to load the profiler module. No profiler data will be available");
        };

        let connection = Connection::connect(&context, context_properties, remote)?;

        let registry = connection.core().get_registry()?;

        // Context needs to be moved to the loop listener
        // but must outlive it to prevent resource leaks
        Ok((mainloop, Rc::new(context), connection, Rc::new(registry)))
    })() {
        Ok(instance) => instance,
        Err(e) => {
            use crate::backend::connection::Error;
            match e {
                Error::PipeWire(e) => {
                    eprintln!("Error initializing PipeWire: {e}");
                }
                #[cfg(feature = "xdg_desktop_portals")]
                Error::PortalUnavailable => {
                    eprintln!("Portal unavailable");
                }
                #[cfg(feature = "xdg_desktop_portals")]
                Error::Ashpd(e) => {
                    eprintln!("Error accessing portal: {e}")
                }
            }
            sx.send(Event::Stop).ok();
            return;
        }
    };
    let core = connection.core();

    let binds = Rc::new(RefCell::new(HashMap::<u32, BoundGlobal>::new()));

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
                if let Some(object) = binds.borrow().get(&id) {
                    object.call(method);
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
                #[cfg(feature = "pw_v0_3_77")]
                if REMOTE_VERSION.get().is_none() {
                    let mut version = info.version().split('.').filter_map(|v| v.parse().ok());

                    if let (Some(major), Some(minor), Some(micro)) =
                        (version.next(), version.next(), version.next())
                    {
                        REMOTE_VERSION.set((major, minor, micro)).ok();
                    }
                }

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
            let mainloop = mainloop.clone();
            move |id, _, res, msg| {
                eprintln!("Core: Error on proxy {id}: {res} - {msg}");

                // -EPIPE on the core proxy usually means the remote has been closed
                if id == 0 && res == -32 {
                    mainloop.quit();
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
                        Error::Unimplemented => {
                            eprintln!("Unsupported object type {}", global.type_);
                        }
                        Error::PipeWire(e) => {
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
