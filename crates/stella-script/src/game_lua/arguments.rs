//! Lua argument boundaries recovered from Purple's generated and hand-written members.

mod conversion;
mod diagnostics;
mod lua51;
mod strict;
mod table;
mod value;

pub(crate) use conversion::native_fcvtzs_f32;
pub(crate) use diagnostics::{describe_value, trace_object_loader};
pub(crate) use lua51::{native_lua51_number, native_lua51_string};
pub(crate) use strict::{
    native_required_boolean, native_required_integer, native_required_number,
    native_required_string, native_required_table,
};
pub(crate) use table::{table_required_number, table_required_string};
pub(crate) use value::{
    native_integer, value_bool, value_number, value_number_at, value_string, value_table,
};
