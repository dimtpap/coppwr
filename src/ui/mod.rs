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

mod globals_store;
mod metadata_editor;
mod module_loader;
mod object_creator;
mod profiler;
mod tool;

use globals_store::GlobalsStore;
use metadata_editor::MetadataEditor;
use module_loader::ModuleLoader;
use object_creator::ObjectCreator;
use profiler::Profiler;
use tool::{Tool, WindowedTool};

mod app;
pub use app::CoppwrApp;
