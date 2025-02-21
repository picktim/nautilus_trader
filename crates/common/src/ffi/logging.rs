// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2025 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
//  You may not use this file except in compliance with the License.
//  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.
// -------------------------------------------------------------------------------------------------

use std::{
    ffi::c_char,
    ops::{Deref, DerefMut},
};

use nautilus_core::{
    UUID4,
    ffi::{
        parsing::{optional_bytes_to_json, u8_as_bool},
        string::{cstr_as_str, cstr_to_ustr, optional_cstr_to_str},
    },
};
use nautilus_model::identifiers::TraderId;

use crate::{
    enums::{LogColor, LogLevel},
    logging::{
        self, headers,
        logger::{self, LogGuard, LoggerConfig},
        logging_set_bypass, map_log_level_to_filter, parse_component_levels,
        writer::FileWriterConfig,
    },
};

/// C compatible Foreign Function Interface (FFI) for an underlying [`LogGuard`].
///
/// This struct wraps `LogGuard` in a way that makes it compatible with C function
/// calls, enabling interaction with `LogGuard` in a C environment.
///
/// It implements the `Deref` trait, allowing instances of `LogGuard_API` to be
/// dereferenced to `LogGuard`, providing access to `LogGuard`'s methods without
/// having to manually access the underlying `LogGuard` instance.
#[repr(C)]
#[allow(non_camel_case_types)]
pub struct LogGuard_API(Box<LogGuard>);

impl Deref for LogGuard_API {
    type Target = LogGuard;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for LogGuard_API {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Initializes logging.
///
/// Logging should be used for Python and sync Rust logic which is most of
/// the components in the main `nautilus_trader` package.
/// Logging can be configured to filter components and write up to a specific level only
/// by passing a configuration using the `NAUTILUS_LOG` environment variable.
///
/// # Safety
///
/// Should only be called once during an applications run, ideally at the
/// beginning of the run.
///
/// - Assume `directory_ptr` is either NULL or a valid C string pointer.
/// - Assume `file_name_ptr` is either NULL or a valid C string pointer.
/// - Assume `file_format_ptr` is either NULL or a valid C string pointer.
/// - Assume `component_level_ptr` is either NULL or a valid C string pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn logging_init(
    trader_id: TraderId,
    instance_id: UUID4,
    level_stdout: LogLevel,
    level_file: LogLevel,
    directory_ptr: *const c_char,
    file_name_ptr: *const c_char,
    file_format_ptr: *const c_char,
    component_levels_ptr: *const c_char,
    is_colored: u8,
    is_bypassed: u8,
    print_config: u8,
) -> LogGuard_API {
    let level_stdout = map_log_level_to_filter(level_stdout);
    let level_file = map_log_level_to_filter(level_file);

    let component_levels_json = unsafe { optional_bytes_to_json(component_levels_ptr) };
    let component_levels = parse_component_levels(component_levels_json);

    let config = LoggerConfig::new(
        level_stdout,
        level_file,
        component_levels,
        u8_as_bool(is_colored),
        u8_as_bool(print_config),
    );

    let directory =
        unsafe { optional_cstr_to_str(directory_ptr).map(std::string::ToString::to_string) };
    let file_name =
        unsafe { optional_cstr_to_str(file_name_ptr).map(std::string::ToString::to_string) };
    let file_format =
        unsafe { optional_cstr_to_str(file_format_ptr).map(std::string::ToString::to_string) };
    let file_config = FileWriterConfig::new(directory, file_name, file_format);

    if u8_as_bool(is_bypassed) {
        logging_set_bypass();
    }

    LogGuard_API(Box::new(logging::init_logging(
        trader_id,
        instance_id,
        config,
        file_config,
    )))
}

/// Creates a new log event.
///
/// # Safety
///
/// - Assumes `component_ptr` is a valid C string pointer.
/// - Assumes `message_ptr` is a valid C string pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn logger_log(
    level: LogLevel,
    color: LogColor,
    component_ptr: *const c_char,
    message_ptr: *const c_char,
) {
    let component = unsafe { cstr_to_ustr(component_ptr) };
    let message = unsafe { cstr_as_str(message_ptr) };

    logger::log(level, color, component, message);
}

/// Logs the Nautilus system header.
///
/// # Safety
///
/// - Assumes `machine_id_ptr` is a valid C string pointer.
/// - Assumes `component_ptr` is a valid C string pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn logging_log_header(
    trader_id: TraderId,
    machine_id_ptr: *const c_char,
    instance_id: UUID4,
    component_ptr: *const c_char,
) {
    let component = unsafe { cstr_to_ustr(component_ptr) };
    let machine_id = unsafe { cstr_as_str(machine_id_ptr) };
    headers::log_header(trader_id, machine_id, instance_id, component);
}

/// Logs system information.
///
/// # Safety
///
/// - Assumes `component_ptr` is a valid C string pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn logging_log_sysinfo(component_ptr: *const c_char) {
    let component = unsafe { cstr_to_ustr(component_ptr) };
    headers::log_sysinfo(component);
}

/// Flushes global logger buffers of any records.
#[unsafe(no_mangle)]
pub extern "C" fn logger_drop(log_guard: LogGuard_API) {
    drop(log_guard);
}
