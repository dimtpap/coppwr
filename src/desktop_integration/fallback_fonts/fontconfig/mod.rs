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

use std::{
    ffi::{CStr, c_int},
    mem::MaybeUninit,
    ops::Deref,
    ptr::{self, NonNull},
};

pub mod ffi;

pub use ffi::{FcFini as fini, FcInit as init};

pub mod properties {
    use super::ffi;

    pub use ffi::FC_FAMILY as FAMILY;
    pub use ffi::FC_FILE as FILE;
    pub use ffi::FC_LANG as LANG;
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct Result(ffi::FcResult);

impl Result {
    pub const MATCH: Self = Self(ffi::FcResult_FcResultMatch);
    pub const NO_MATCH: Self = Self(ffi::FcResult_FcResultNoMatch);
    pub const TYPE_MISMATCH: Self = Self(ffi::FcResult_FcResultTypeMismatch);
    pub const NO_ID: Self = Self(ffi::FcResult_FcResultNoId);
    pub const OUT_OF_MEMORY: Self = Self(ffi::FcResult_FcResultOutOfMemory);

    const fn from_raw(raw: ffi::FcResult) -> Self {
        Self(raw)
    }
}

impl std::fmt::Display for Result {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::MATCH => f.write_str("Match"),
            Self::NO_MATCH => f.write_str("No match"),
            Self::TYPE_MISMATCH => f.write_str("Type mismatch"),
            Self::NO_ID => f.write_str("No ID"),
            Self::OUT_OF_MEMORY => f.write_str("Out of memory"),
            Self(other) => write!(f, "Unknown: {other}"),
        }
    }
}

impl std::error::Error for Result {}

#[derive(PartialEq, Eq, Clone, Copy)]
pub struct MatchKind(ffi::FcMatchKind);

impl MatchKind {
    pub const PATTERN: Self = Self(ffi::FcMatchKind_FcMatchPattern);
    pub const FONT: Self = Self(ffi::FcMatchKind_FcMatchFont);
    pub const SCAN: Self = Self(ffi::FcMatchKind_FcMatchScan);

    const fn as_raw(self) -> ffi::FcMatchKind {
        self.0
    }
}

impl std::fmt::Debug for MatchKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "MatchKind::{}",
            match *self {
                Self::PATTERN => "Pattern",
                Self::FONT => "Font",
                Self::SCAN => "Scan",
                _ => "Unknown",
            }
        )
    }
}

#[repr(transparent)]
pub struct PatternRef(ffi::FcPattern);

impl PatternRef {
    const fn as_raw_ptr(&self) -> *const ffi::FcPattern {
        &raw const self.0
    }

    pub fn get_string(
        &self,
        object: impl AsRef<[ffi::FcChar8]>,
        n: c_int,
    ) -> std::result::Result<Option<&CStr>, Result> {
        let mut str = MaybeUninit::uninit();
        let res = unsafe {
            ffi::FcPatternGetString(
                self.as_raw_ptr(),
                object.as_ref().as_ptr().cast(),
                n,
                str.as_mut_ptr(),
            )
        };

        let str = unsafe { str.assume_init() };
        let res = Result::from_raw(res);

        if res != Result::MATCH {
            Err(res)
        } else if str.is_null() {
            Ok(None)
        } else {
            Ok(Some(unsafe { CStr::from_ptr(str.cast()) }))
        }
    }

    pub fn get_langset(
        &self,
        object: impl AsRef<[ffi::FcChar8]>,
        n: c_int,
    ) -> std::result::Result<Option<&LangSetRef>, Result> {
        let mut ls = MaybeUninit::<*mut ffi::FcLangSet>::uninit();

        let res = unsafe {
            ffi::FcPatternGetLangSet(
                self.as_raw_ptr(),
                object.as_ref().as_ptr().cast(),
                n,
                ls.as_mut_ptr(),
            )
        };

        let ls = unsafe { ls.assume_init() };
        let res = Result::from_raw(res);

        if res == Result::MATCH {
            unsafe { Ok(ls.cast::<LangSetRef>().as_ref()) }
        } else {
            Err(res)
        }
    }
}

#[repr(transparent)]
#[derive(Debug)]
pub struct Pattern(NonNull<ffi::FcPattern>);

impl Pattern {
    pub fn new() -> Self {
        unsafe { Self(NonNull::new(ffi::FcPatternCreate()).unwrap()) }
    }

    pub fn add_langset(&mut self, object: &[u8], ls: &LangSet) -> bool {
        unsafe {
            ffi::FcPatternAddLangSet(
                self.as_raw_ptr().cast_mut(),
                object.as_ptr().cast(),
                ls.as_raw_ptr(),
            )
            .try_into()
            .unwrap()
        }
    }

    pub fn default_substitute(&mut self) {
        unsafe {
            ffi::FcDefaultSubstitute(self.as_raw_ptr().cast_mut());
        }
    }
}

impl Deref for Pattern {
    type Target = PatternRef;

    fn deref(&self) -> &Self::Target {
        unsafe { self.0.cast().as_ref() }
    }
}

impl Drop for Pattern {
    fn drop(&mut self) {
        unsafe { ffi::FcPatternDestroy(self.as_raw_ptr().cast_mut()) }
    }
}

#[repr(transparent)]
pub struct Config(*mut ffi::FcConfig);

impl Default for Config {
    fn default() -> Self {
        Self(ptr::null_mut())
    }
}

impl Config {
    const fn as_raw_ptr(&self) -> *mut ffi::FcConfig {
        self.0
    }

    pub fn substitute(&mut self, p: &mut Pattern, kind: MatchKind) -> ffi::FcBool {
        unsafe {
            ffi::FcConfigSubstitute(self.as_raw_ptr(), p.as_raw_ptr().cast_mut(), kind.as_raw())
        }
    }

    pub fn font_sort(&mut self, p: &mut Pattern, trim: bool) -> (Option<FontSet>, Result) {
        let mut res = MaybeUninit::uninit();

        let set = unsafe {
            ffi::FcFontSort(
                self.as_raw_ptr(),
                p.as_raw_ptr().cast_mut(),
                trim as ffi::FcBool,
                std::ptr::null_mut(),
                res.as_mut_ptr(),
            )
        };

        let res = Result::from_raw(unsafe { res.assume_init() });

        (FontSet::from_raw(set), res)
    }
}

impl Drop for Config {
    fn drop(&mut self) {
        let ptr = self.as_raw_ptr();
        if !ptr.is_null() {
            unsafe { ffi::FcConfigDestroy(ptr) }
        }
    }
}

#[repr(transparent)]
pub struct LangSetRef(ffi::FcLangSet);

impl LangSetRef {
    const fn as_raw_ptr(&self) -> *const ffi::FcLangSet {
        &raw const self.0
    }

    pub fn subtract(&self, lsb: &Self) -> LangSet {
        unsafe {
            LangSet::from_raw(ffi::FcLangSetSubtract(self.as_raw_ptr(), lsb.as_raw_ptr())).unwrap()
        }
    }
}

impl PartialEq for LangSetRef {
    fn eq(&self, other: &Self) -> bool {
        unsafe {
            ffi::FcLangSetEqual(self.as_raw_ptr(), other.as_raw_ptr())
                .try_into()
                .unwrap()
        }
    }
}

#[repr(transparent)]
pub struct LangSet(NonNull<ffi::FcLangSet>);

impl LangSet {
    unsafe fn from_raw(raw: *mut ffi::FcLangSet) -> Option<Self> {
        NonNull::new(raw).map(Self)
    }

    pub fn new() -> Self {
        unsafe { Self(NonNull::new(ffi::FcLangSetCreate()).unwrap()) }
    }

    pub fn add(&mut self, lang: &CStr) -> bool {
        unsafe {
            ffi::FcLangSetAdd(self.as_raw_ptr().cast_mut(), lang.as_ptr().cast())
                .try_into()
                .unwrap()
        }
    }
}

impl<'a> FromIterator<&'a CStr> for LangSet {
    fn from_iter<T: IntoIterator<Item = &'a CStr>>(iter: T) -> Self {
        let mut ls = Self::new();

        for s in iter {
            ls.add(s);
        }

        ls
    }
}

impl std::ops::Deref for LangSet {
    type Target = LangSetRef;

    fn deref(&self) -> &Self::Target {
        unsafe { self.0.cast().as_ref() }
    }
}

impl PartialEq for LangSet {
    fn eq(&self, other: &Self) -> bool {
        self.deref() == other.deref()
    }
}

impl Drop for LangSet {
    fn drop(&mut self) {
        unsafe { ffi::FcLangSetDestroy(self.as_raw_ptr().cast_mut()) }
    }
}

#[repr(transparent)]
pub struct FontSet(NonNull<ffi::FcFontSet>);

impl FontSet {
    const fn as_raw_ptr(&self) -> *mut ffi::FcFontSet {
        self.0.as_ptr()
    }

    fn from_raw(raw: *mut ffi::FcFontSet) -> Option<Self> {
        NonNull::new(raw).map(Self)
    }

    pub fn len(&self) -> usize {
        unsafe { self.0.as_ref() }.nfont.try_into().unwrap()
    }

    pub fn fonts(&self) -> &[&PatternRef] {
        unsafe {
            let raw = self.0.as_ref();

            let data = raw.fonts as *const &PatternRef;

            std::slice::from_raw_parts(data, self.len())
        }
    }
}

impl Drop for FontSet {
    fn drop(&mut self) {
        unsafe {
            ffi::FcFontSetDestroy(self.as_raw_ptr());
        }
    }
}
