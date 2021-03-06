// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license OR Apache 2.0
#![feature(nll)]
#![feature(async_closure)]
#![feature(label_break_value)]
#![feature(never_type)]
#![feature(str_split_once)]
#![feature(try_blocks)]
#![feature(type_ascription)]
#![allow(dead_code)]
#![allow(unused_imports)]

pub mod common;
pub mod util;

pub mod client;
pub mod server;

#[cfg(feature = "ffi")]
pub mod ffi;
