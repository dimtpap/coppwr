// Copyright 2023-2026 Dimitris Papaioannou <dimtpap@protonmail.com>
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

use std::{borrow::Cow, ffi::OsStr, io, os::unix::ffi::OsStrExt as _};

use egui::{
    FontTweak,
    epaint::text::{FontData, FontFamily, FontInsert, FontPriority, InsertFontFamily},
};
use read_fonts::{FontRef, ReadError, TableProvider as _, tables::name::NameId};

mod fontconfig;
use fontconfig as fc;

#[derive(Debug)]
pub enum Error {
    FontconfigNoMatches,
    FontRead(io::Error),
    ReadFontsParse(ReadError),
    ReadFontsMissingNameTable(ReadError),
    ReadFontsMissingName,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FontconfigNoMatches => f.write_str("Fontconfig didn't match pattern"),
            Self::FontRead(e) => write!(f, "Cannot read font file: {e}"),
            Self::ReadFontsParse(e) => write!(f, "Cannot parse font data: {e}"),
            Self::ReadFontsMissingNameTable(e) => write!(f, "Font data has no name table: {e}"),
            Self::ReadFontsMissingName => f.write_str("Font name table has no full name"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::FontconfigNoMatches | Self::ReadFontsMissingName => None,
            Self::FontRead(e) => Some(e),
            Self::ReadFontsParse(e) | Self::ReadFontsMissingNameTable(e) => Some(e),
        }
    }
}

fn add_font(ctx: &egui::Context, font: &FontRef<'static>) -> Result<(), Error> {
    let name_table = font.name().map_err(Error::ReadFontsMissingNameTable)?;

    let name = name_table
        .name_record()
        .iter()
        .find(|r| r.name_id == NameId::FULL_NAME)
        .map(|r| r.string(name_table.string_data()).unwrap().to_string())
        .ok_or(Error::ReadFontsMissingName)?;

    let width = font
        .post()
        .map(|p| {
            if p.is_fixed_pitch() == 0 {
                FontFamily::Proportional
            } else {
                FontFamily::Monospace
            }
        })
        .unwrap_or_default();

    eprintln!("Adding fallback font {name} {width:?}");

    let fi = FontInsert {
        name,
        data: FontData {
            font: Cow::Borrowed(font.data().as_bytes()),
            index: font.ttc_index().unwrap_or(0),
            tweak: FontTweak::default(),
        },
        families: vec![InsertFontFamily {
            family: width,
            priority: FontPriority::Lowest,
        }],
    };
    ctx.add_font(fi);

    Ok(())
}

pub fn add_fallback_fonts(ctx: &egui::Context) -> Result<(), Error> {
    struct FcFiniGuard;
    impl Drop for FcFiniGuard {
        fn drop(&mut self) {
            unsafe {
                fc::fini();
            }
        }
    }

    unsafe {
        fc::init();
    }
    let _g = FcFiniGuard;

    let langs = [c"ja", c"ko", c"zh-cn", c"zh-tw"];

    let mut pattern = fc::Pattern::new();

    // Langs have to be added as separate lang sets in order
    // for Fontconfig to properly sort fonts
    for lang in langs {
        let mut ls = fc::LangSet::new();
        ls.add(lang);
        pattern.add_langset(fc::properties::LANG, &ls);
    }

    let mut langs = fc::LangSet::from_iter(langs);

    let mut config = fc::Config::default();
    config.substitute(&mut pattern, fc::MatchKind::PATTERN);
    pattern.default_substitute();

    let s = config
        .font_sort(&mut pattern, true)
        .0
        .ok_or(Error::FontconfigNoMatches)?;

    for p in s.fonts() {
        let Ok(Some(font_langs)) = p.get_langset(fc::properties::LANG, 0) else {
            continue;
        };

        let missing_langs = langs.subtract(font_langs);
        if missing_langs == langs {
            // Font did not contain remaining langs.
            // Since they're sorted, neither will the next ones.
            break;
        }
        langs = missing_langs;

        let name = p.get_string(fc::properties::FAMILY, 0).ok().flatten();

        let font_path = match p.get_string(fc::properties::FILE, 0) {
            Ok(Some(p)) => OsStr::from_bytes(p.to_bytes()),
            Ok(None) => {
                eprintln!("Fontconfig font {name:?} has no path");
                continue;
            }
            Err(e) => {
                eprintln!("Cannot get fontconfig font {name:?} path: {e}");
                continue;
            }
        };

        // Could use Fontconfig's FcFreeType* functions, but each call to those
        // will read the font file. Instead read the file into a buffer once and
        // use read-fonts on that and with egui.

        // `Vec::leak` can be used here since the font exists for the runtime of
        // the program and to allow the same data to be shared between fonts in collections.
        let font_data = std::fs::read(font_path)
            .map(Vec::leak)
            .map_err(Error::FontRead)? as &_;

        let font = read_fonts::FileRef::new(font_data).map_err(Error::ReadFontsParse)?;
        match font {
            read_fonts::FileRef::Collection(col) => {
                for (i, font) in col.iter().enumerate() {
                    let font = match font {
                        Ok(f) => f,
                        Err(e) => {
                            eprintln!("Cannot parse font {i} from collection: {e}");
                            continue;
                        }
                    };

                    if let Err(e) = add_font(ctx, &font) {
                        eprintln!("Cannot add font from collection: {e}");
                    }
                }
            }
            read_fonts::FileRef::Font(font) => {
                if let Err(e) = add_font(ctx, &font) {
                    eprintln!("Cannot add font: {e}");
                }
            }
        }
    }

    Ok(())
}
