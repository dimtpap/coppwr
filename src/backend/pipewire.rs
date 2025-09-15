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

use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::mpsc};

use crate::backend::connection;

use super::{
    Connection, Event, RemoteInfo, Request,
    bind::BoundGlobal,
    pw::{self, proxy::ProxyT, types::ObjectType},
    util,
};

#[cfg(feature = "pw_v0_3_77")]
use super::REMOTE_VERSION;

pub fn pipewire_thread(
    remote: RemoteInfo,
    mainloop_properties: Vec<(String, String)>,
    context_properties: Vec<(String, String)>,
    sx: mpsc::Sender<Event>,
    pwrx: pw::channel::Receiver<Request>,
) {
    // Proxies created by core.create_object
    #[allow(dead_code)] // The fields are never read from
    struct LocalProxy(pw::proxy::Proxy, pw::proxy::ProxyListener);

    let send = move |event| {
        // It is ok to ignore the error here since `backend::Handle` makes it
        // impossible to drop the receiver without also stopping the thread
        _ = sx.send(event);
    };

    // Proxies created by core.create_object are kept seperate from proxies created
    // by registry binding because they've not been bound yet and need to be kept alive
    // until they become available in the registry and object listeners can be added on them
    let locals = Rc::new(RefCell::new(HashMap::new()));
    let binds = Rc::new(RefCell::new(HashMap::<u32, BoundGlobal>::new()));

    let (mainloop, context, connection, registry): (
        pw::main_loop::MainLoopRc,
        pw::context::ContextRc,
        Connection,
        pw::registry::RegistryRc,
    ) = {
        let result = (|| -> Result<_, connection::Error> {
            let mainloop = if mainloop_properties.is_empty() {
                pw::main_loop::MainLoopRc::new(None)?
            } else {
                pw::main_loop::MainLoopRc::new(Some(
                    util::key_val_to_props(mainloop_properties.into_iter()).dict(),
                ))?
            };

            let context = pw::context::ContextRc::new(&mainloop, None)?;
            if context
                .load_module("libpipewire-module-profiler", None, None)
                .is_err()
            {
                eprintln!("Failed to load the profiler module. No profiler data will be available");
            }

            let connection = Connection::open(&context, context_properties, remote)?;

            let registry = connection.core().get_registry_rc()?;

            Ok((mainloop, context, connection, registry))
        })();

        match result {
            Ok(instance) => instance,
            Err(e) => {
                eprintln!("Failed to connect to remote: {e}");

                send(Event::Stop);

                return;
            }
        }
    };
    let core = connection.core();

    let _receiver = pwrx.attach(mainloop.loop_(), {
        let send = send.clone();
        let mainloop = mainloop.clone();
        let context = context.clone();
        let core = core.clone();
        let registry = registry.clone();

        let locals = Rc::clone(&locals);
        let binds = Rc::clone(&binds);

        move |msg| match msg {
            Request::Stop => {
                mainloop.quit();
            }
            Request::CreateObject(object_type, factory, props) => {
                let props = util::key_val_to_props(props.into_iter());

                let proxy = match object_type {
                    ObjectType::Link => core
                        .create_object::<pw::link::Link>(factory.as_str(), &props)
                        .map(ProxyT::upcast),
                    ObjectType::Port => core
                        .create_object::<pw::port::Port>(factory.as_str(), &props)
                        .map(ProxyT::upcast),
                    ObjectType::Node => core
                        .create_object::<pw::node::Node>(factory.as_str(), &props)
                        .map(ProxyT::upcast),
                    ObjectType::Client => core
                        .create_object::<pw::client::Client>(factory.as_str(), &props)
                        .map(ProxyT::upcast),
                    ObjectType::Device => core
                        .create_object::<pw::device::Device>(factory.as_str(), &props)
                        .map(ProxyT::upcast),
                    ObjectType::Factory => core
                        .create_object::<pw::factory::Factory>(factory.as_str(), &props)
                        .map(ProxyT::upcast),
                    ObjectType::Metadata => core
                        .create_object::<pw::metadata::Metadata>(factory.as_str(), &props)
                        .map(ProxyT::upcast),
                    ObjectType::Module => core
                        .create_object::<pw::module::Module>(factory.as_str(), &props)
                        .map(ProxyT::upcast),
                    ObjectType::Profiler => core
                        .create_object::<pw::profiler::Profiler>(factory.as_str(), &props)
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
                name,
                args,
                props,
            } => {
                let props = props.map(|props| util::key_val_to_props(props.into_iter()));

                if let Err(e) = context
                    .load_module(name.as_str(), args.as_deref(), props)
                {
                    eprintln!("Failed to load module (Name: {name} - Arguments: {args:?}): {e}");
                }
            }
            Request::GetContextProperties => {
                send(Event::ContextProperties(util::dict_to_map(context.properties().dict())));
            }
            Request::UpdateContextProperties(props) => {
                context.update_properties(util::key_val_to_props(props.into_iter()).dict());
            }
            Request::CallObjectMethod(id, method) => {
                if let Some(object) = binds.borrow().get(&id) {
                    object.call(method);
                }
            }
        }
    });

    send(Event::GlobalAdded(0, ObjectType::Core, None));

    let _core_listener = core
        .add_listener_local()
        .info({
            let send = send.clone();
            move |info| {
                #[cfg(feature = "pw_v0_3_77")]
                {
                    let mut rv = REMOTE_VERSION.lock().unwrap();
                    if rv.is_none() {
                        let mut version = info.version().split('.').filter_map(|v| v.parse().ok());

                        if let (Some(major), Some(minor), Some(patch)) =
                            (version.next(), version.next(), version.next())
                        {
                            *rv = Some((major, minor, patch));
                        }
                    }
                }

                let infos = Box::new([
                    ("Name", info.name().to_owned()),
                    ("Hostname", info.host_name().to_owned()),
                    ("Username", info.user_name().to_owned()),
                    ("Version", info.version().to_owned()),
                    ("Cookie", info.cookie().to_string()),
                ]);

                send(Event::GlobalInfo(0, infos));

                if let (true, Some(props)) = (
                    info.change_mask().contains(pw::core::ChangeMask::PROPS),
                    info.props(),
                ) {
                    send(Event::GlobalProperties(0, util::dict_to_map(props)));
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
            let send = send.clone();
            let registry = registry.clone();
            let binds = Rc::clone(&binds);
            move |global| {
                if global.id == 0 {
                    return;
                }

                send(Event::GlobalAdded(
                    global.id,
                    global.type_.clone(),
                    global.props.map(util::dict_to_map),
                ));

                let id = global.id;
                let proxy_removed = {
                    let binds = binds.clone();
                    move || {
                        binds.borrow_mut().remove(&id);
                    }
                };
                match BoundGlobal::bind_to(&registry, global, send.clone(), proxy_removed) {
                    Ok(bound_global) => {
                        binds.borrow_mut().insert(id, bound_global);
                    }
                    Err(e) => {
                        eprintln!("Error binding object {id}: {e}");
                    }
                }
            }
        })
        .global_remove({
            let send = send.clone();
            move |id| {
                send(Event::GlobalRemoved(id));
            }
        })
        .register();

    send(Event::ContextProperties(util::dict_to_map(
        context.properties().dict(),
    )));

    mainloop.run();

    send(Event::Stop);
}
