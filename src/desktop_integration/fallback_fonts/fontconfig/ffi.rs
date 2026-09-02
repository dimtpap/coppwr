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

#![allow(non_upper_case_globals)]

use std::ffi::{c_char, c_int, c_uchar, c_uint};

pub type FcChar8 = c_uchar;
pub type FcBool = c_int;

#[repr(C)]
pub struct FcPattern([u8; 0]);

#[repr(C)]
pub struct FcConfig([u8; 0]);

#[repr(C)]
pub struct FcCharSet([u8; 0]);

#[repr(C)]
pub struct FcLangSet([u8; 0]);

#[repr(C)]
pub struct FcFontSet {
    pub nfont: c_int,
    pub sfont: c_int,
    pub fonts: *mut *mut FcPattern,
}

pub const FC_FAMILY: &[u8; 7] = b"family\0";
pub const FC_FILE: &[u8; 5] = b"file\0";
pub const FC_LANG: &[u8; 5] = b"lang\0";

pub type FcResult = c_uint;
pub const FcResult_FcResultMatch: FcResult = 0;
pub const FcResult_FcResultNoMatch: FcResult = 1;
pub const FcResult_FcResultTypeMismatch: FcResult = 2;
pub const FcResult_FcResultNoId: FcResult = 3;
pub const FcResult_FcResultOutOfMemory: FcResult = 4;

pub type FcMatchKind = c_uint;
pub const FcMatchKind_FcMatchPattern: FcMatchKind = 0;
pub const FcMatchKind_FcMatchFont: FcMatchKind = 1;
pub const FcMatchKind_FcMatchScan: FcMatchKind = 2;
#[allow(unused)]
pub const FcMatchKind_FcMatchKindEnd: FcMatchKind = 3;
#[allow(unused)]
pub const FcMatchKind_FcMatchKindBegin: FcMatchKind = FcMatchKind_FcMatchPattern;

#[link(name = "fontconfig")]
unsafe extern "C" {
    pub unsafe fn FcInit() -> FcBool;
    pub unsafe fn FcFini();

    pub unsafe fn FcPatternCreate() -> *mut FcPattern;
    pub unsafe fn FcPatternAddLangSet(
        p: *mut FcPattern,
        object: *const c_char,
        ls: *const FcLangSet,
    ) -> FcBool;
    pub unsafe fn FcPatternGetString(
        p: *const FcPattern,
        object: *const c_char,
        n: c_int,
        s: *mut *mut FcChar8,
    ) -> FcResult;
    pub unsafe fn FcPatternGetLangSet(
        p: *const FcPattern,
        object: *const c_char,
        n: c_int,
        ls: *mut *mut FcLangSet,
    ) -> FcResult;
    pub unsafe fn FcPatternDestroy(p: *mut FcPattern);

    pub unsafe fn FcConfigSetDefaultSubstitute(config: *mut FcConfig, pattern: *mut FcPattern);
    pub unsafe fn FcConfigSubstitute(
        config: *mut FcConfig,
        p: *mut FcPattern,
        kind: FcMatchKind,
    ) -> FcBool;
    pub unsafe fn FcConfigDestroy(config: *mut FcConfig);

    pub unsafe fn FcFontSort(
        config: *mut FcConfig,
        p: *mut FcPattern,
        trim: FcBool,
        csp: *mut *mut FcCharSet,
        result: *mut FcResult,
    ) -> *mut FcFontSet;

    pub unsafe fn FcLangSetCreate() -> *mut FcLangSet;
    pub unsafe fn FcLangSetAdd(ls: *mut FcLangSet, lang: *const FcChar8) -> FcBool;
    pub unsafe fn FcLangSetSubtract(lsa: *const FcLangSet, lsb: *const FcLangSet)
    -> *mut FcLangSet;
    pub unsafe fn FcLangSetEqual(lsa: *const FcLangSet, lsb: *const FcLangSet) -> FcBool;
    pub unsafe fn FcLangSetDestroy(ls: *mut FcLangSet);

    pub unsafe fn FcFontSetDestroy(s: *mut FcFontSet);
}
