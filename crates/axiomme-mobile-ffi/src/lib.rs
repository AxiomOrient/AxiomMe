use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::Path;
use std::ptr;

use axiomme_core::{AxiomError, AxiomMe, AxiomUri, Scope};
use serde::Serialize;

/// Stable return codes for C/Swift callers.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxiommeFfiCode {
    Ok = 0,
    InvalidArgument = 1,
    RuntimeError = 2,
}

/// Owned byte buffer returned across the FFI boundary.
///
/// Ownership:
/// - Producer: Rust allocates (`Box<[u8]>`) and returns pointer/len.
/// - Consumer: Calls `axiomme_owned_bytes_free` exactly once.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AxiommeOwnedBytes {
    pub ptr: *mut u8,
    pub len: usize,
}

impl AxiommeOwnedBytes {
    const fn empty() -> Self {
        Self {
            ptr: ptr::null_mut(),
            len: 0,
        }
    }

    fn from_vec(value: Vec<u8>) -> Self {
        if value.is_empty() {
            return Self::empty();
        }
        let boxed = value.into_boxed_slice();
        let len = boxed.len();
        let ptr = Box::into_raw(boxed) as *mut u8;
        Self { ptr, len }
    }
}

/// Uniform response envelope for every exported call.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AxiommeFfiResult {
    pub code: AxiommeFfiCode,
    pub payload: AxiommeOwnedBytes,
}

impl AxiommeFfiResult {
    const fn ok_empty() -> Self {
        Self {
            code: AxiommeFfiCode::Ok,
            payload: AxiommeOwnedBytes::empty(),
        }
    }

    fn ok_json_bytes(payload: Vec<u8>) -> Self {
        Self {
            code: AxiommeFfiCode::Ok,
            payload: AxiommeOwnedBytes::from_vec(payload),
        }
    }

    fn invalid_argument(operation: &'static str, message: impl Into<String>) -> Self {
        let payload = FfiArgumentErrorPayload {
            code: "INVALID_ARGUMENT",
            operation,
            message: message.into(),
        };
        Self::json_or_internal(AxiommeFfiCode::InvalidArgument, operation, &payload)
    }

    fn runtime_error(operation: &'static str, err: AxiomError) -> Self {
        let payload = err.to_payload(operation.to_string(), None);
        Self::json_or_internal(AxiommeFfiCode::RuntimeError, operation, &payload)
    }

    fn internal_error(operation: &'static str, message: impl Into<String>) -> Self {
        let payload = FfiArgumentErrorPayload {
            code: "FFI_INTERNAL",
            operation,
            message: message.into(),
        };
        Self::json_or_internal(AxiommeFfiCode::RuntimeError, operation, &payload)
    }

    fn json_or_internal(
        code: AxiommeFfiCode,
        operation: &'static str,
        payload: &impl Serialize,
    ) -> Self {
        match serde_json::to_vec(payload) {
            Ok(json) => Self {
                code,
                payload: AxiommeOwnedBytes::from_vec(json),
            },
            Err(err) => {
                let fallback = format!(
                    r#"{{"code":"FFI_INTERNAL","operation":"{operation}","message":"failed to serialize payload: {err}"}}"#
                )
                .into_bytes();
                Self {
                    code: AxiommeFfiCode::RuntimeError,
                    payload: AxiommeOwnedBytes::from_vec(fallback),
                }
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct FfiArgumentErrorPayload<'a> {
    code: &'static str,
    operation: &'a str,
    message: String,
}

/// Opaque runtime handle for mobile callers.
pub struct AxiommeRuntime {
    app: AxiomMe,
}

#[unsafe(no_mangle)]
/// Create a new runtime handle.
///
/// # Safety
/// - `root_dir` must point to a valid NUL-terminated UTF-8 string.
/// - `out_runtime` must be non-null and writable.
/// - Caller must eventually pass returned runtime to `axiomme_runtime_free`.
pub unsafe extern "C" fn axiomme_runtime_new(
    root_dir: *const c_char,
    out_runtime: *mut *mut AxiommeRuntime,
) -> AxiommeFfiResult {
    const OPERATION: &str = "runtime.new";

    if out_runtime.is_null() {
        return AxiommeFfiResult::invalid_argument(OPERATION, "out_runtime pointer is null");
    }

    let root_dir = match parse_required_c_string(root_dir, OPERATION, "root_dir") {
        Ok(value) => value,
        Err(result) => return result,
    };

    match AxiomMe::new(&root_dir) {
        Ok(app) => {
            let runtime = Box::new(AxiommeRuntime { app });
            // SAFETY: `out_runtime` was validated as non-null and points to writable caller memory.
            unsafe {
                *out_runtime = Box::into_raw(runtime);
            }
            AxiommeFfiResult::ok_empty()
        }
        Err(err) => AxiommeFfiResult::runtime_error(OPERATION, err),
    }
}

#[unsafe(no_mangle)]
/// Initialize runtime directories/index state.
///
/// # Safety
/// - `runtime` must be a live pointer previously returned by `axiomme_runtime_new`.
/// - `runtime` must not be used concurrently without external synchronization.
pub unsafe extern "C" fn axiomme_runtime_initialize(
    runtime: *mut AxiommeRuntime,
) -> AxiommeFfiResult {
    const OPERATION: &str = "runtime.initialize";

    let runtime = match runtime_from_ptr(runtime, OPERATION) {
        Ok(runtime) => runtime,
        Err(result) => return result,
    };

    match runtime.app.initialize() {
        Ok(()) => AxiommeFfiResult::ok_empty(),
        Err(err) => AxiommeFfiResult::runtime_error(OPERATION, err),
    }
}

#[unsafe(no_mangle)]
/// Read backend status and return it as JSON bytes.
///
/// # Safety
/// - `runtime` must be a live pointer previously returned by `axiomme_runtime_new`.
/// - `runtime` must not be used concurrently without external synchronization.
pub unsafe extern "C" fn axiomme_runtime_backend_status_json(
    runtime: *mut AxiommeRuntime,
) -> AxiommeFfiResult {
    const OPERATION: &str = "runtime.backend_status_json";

    let runtime = match runtime_from_ptr(runtime, OPERATION) {
        Ok(runtime) => runtime,
        Err(result) => return result,
    };

    let status = match runtime.app.backend_status() {
        Ok(status) => status,
        Err(err) => return AxiommeFfiResult::runtime_error(OPERATION, err),
    };
    json_success(OPERATION, &status)
}

#[unsafe(no_mangle)]
/// Create a directory in an allowed mutable scope.
///
/// # Safety
/// - `runtime` must be a live pointer previously returned by `axiomme_runtime_new`.
/// - `uri` must be a valid NUL-terminated UTF-8 string.
pub unsafe extern "C" fn axiomme_runtime_mkdir(
    runtime: *mut AxiommeRuntime,
    uri: *const c_char,
) -> AxiommeFfiResult {
    const OPERATION: &str = "runtime.mkdir";

    let runtime = match runtime_from_ptr(runtime, OPERATION) {
        Ok(runtime) => runtime,
        Err(result) => return result,
    };
    let uri = match parse_required_c_string(uri, OPERATION, "uri") {
        Ok(value) => value,
        Err(result) => return result,
    };

    match runtime.app.mkdir(&uri) {
        Ok(()) => AxiommeFfiResult::ok_empty(),
        Err(err) => AxiommeFfiResult::runtime_error(OPERATION, err),
    }
}

#[unsafe(no_mangle)]
/// List entries under a URI and return JSON payload.
///
/// # Safety
/// - `runtime` must be a live pointer previously returned by `axiomme_runtime_new`.
/// - `uri` must be a valid NUL-terminated UTF-8 string.
pub unsafe extern "C" fn axiomme_runtime_ls_json(
    runtime: *mut AxiommeRuntime,
    uri: *const c_char,
    recursive: bool,
) -> AxiommeFfiResult {
    const OPERATION: &str = "runtime.ls_json";

    let runtime = match runtime_from_ptr(runtime, OPERATION) {
        Ok(runtime) => runtime,
        Err(result) => return result,
    };
    let uri = match parse_required_c_string(uri, OPERATION, "uri") {
        Ok(value) => value,
        Err(result) => return result,
    };

    match runtime.app.ls(&uri, recursive, true) {
        Ok(entries) => json_success(OPERATION, &entries),
        Err(err) => AxiommeFfiResult::runtime_error(OPERATION, err),
    }
}

#[unsafe(no_mangle)]
/// Load a markdown document and return it as JSON payload.
///
/// # Safety
/// - `runtime` must be a live pointer previously returned by `axiomme_runtime_new`.
/// - `uri` must be a valid NUL-terminated UTF-8 string.
pub unsafe extern "C" fn axiomme_runtime_load_markdown_json(
    runtime: *mut AxiommeRuntime,
    uri: *const c_char,
) -> AxiommeFfiResult {
    const OPERATION: &str = "runtime.load_markdown_json";

    let runtime = match runtime_from_ptr(runtime, OPERATION) {
        Ok(runtime) => runtime,
        Err(result) => return result,
    };
    let uri = match parse_required_c_string(uri, OPERATION, "uri") {
        Ok(value) => value,
        Err(result) => return result,
    };

    match runtime.app.load_markdown(&uri) {
        Ok(document) => json_success(OPERATION, &document),
        Err(err) => AxiommeFfiResult::runtime_error(OPERATION, err),
    }
}

#[unsafe(no_mangle)]
/// Save markdown content and return metadata as JSON.
///
/// Behavior:
/// - update path: saves existing file with optional etag check.
/// - create path: if file is missing and `expected_etag` is null/empty, creates then saves.
///
/// # Safety
/// - `runtime` must be a live pointer previously returned by `axiomme_runtime_new`.
/// - `uri` and `content` must be valid NUL-terminated UTF-8 strings.
/// - `expected_etag` may be null.
pub unsafe extern "C" fn axiomme_runtime_save_markdown_json(
    runtime: *mut AxiommeRuntime,
    uri: *const c_char,
    content: *const c_char,
    expected_etag: *const c_char,
) -> AxiommeFfiResult {
    const OPERATION: &str = "runtime.save_markdown_json";

    let runtime = match runtime_from_ptr(runtime, OPERATION) {
        Ok(runtime) => runtime,
        Err(result) => return result,
    };
    let uri = match parse_required_c_string(uri, OPERATION, "uri") {
        Ok(value) => value,
        Err(result) => return result,
    };
    let content = match parse_c_string_allow_empty(content, OPERATION, "content") {
        Ok(value) => value,
        Err(result) => return result,
    };
    let expected_etag = match parse_optional_c_string(expected_etag, OPERATION, "expected_etag") {
        Ok(value) => value,
        Err(result) => return result,
    };

    match save_markdown_with_create(runtime, &uri, &content, expected_etag.as_deref()) {
        Ok(saved) => json_success(OPERATION, &saved),
        Err(err) => AxiommeFfiResult::runtime_error(OPERATION, err),
    }
}

#[unsafe(no_mangle)]
/// Remove a file/directory by URI.
///
/// # Safety
/// - `runtime` must be a live pointer previously returned by `axiomme_runtime_new`.
/// - `uri` must be a valid NUL-terminated UTF-8 string.
pub unsafe extern "C" fn axiomme_runtime_rm(
    runtime: *mut AxiommeRuntime,
    uri: *const c_char,
    recursive: bool,
) -> AxiommeFfiResult {
    const OPERATION: &str = "runtime.rm";

    let runtime = match runtime_from_ptr(runtime, OPERATION) {
        Ok(runtime) => runtime,
        Err(result) => return result,
    };
    let uri = match parse_required_c_string(uri, OPERATION, "uri") {
        Ok(value) => value,
        Err(result) => return result,
    };

    match runtime.app.rm(&uri, recursive) {
        Ok(()) => AxiommeFfiResult::ok_empty(),
        Err(err) => AxiommeFfiResult::runtime_error(OPERATION, err),
    }
}

#[unsafe(no_mangle)]
/// Destroy a runtime previously created by `axiomme_runtime_new`.
///
/// # Safety
/// - `runtime` must be null or a pointer returned by `axiomme_runtime_new`.
/// - The pointer must be freed exactly once.
pub unsafe extern "C" fn axiomme_runtime_free(runtime: *mut AxiommeRuntime) {
    if runtime.is_null() {
        return;
    }
    // SAFETY: pointer was produced by `Box::into_raw` in `axiomme_runtime_new`.
    unsafe {
        drop(Box::from_raw(runtime));
    }
}

#[unsafe(no_mangle)]
/// Free JSON/byte payload memory returned by FFI calls.
///
/// # Safety
/// - `bytes` must be a value returned by this crate.
/// - The value must be freed exactly once.
pub unsafe extern "C" fn axiomme_owned_bytes_free(bytes: AxiommeOwnedBytes) {
    if bytes.ptr.is_null() || bytes.len == 0 {
        return;
    }
    // SAFETY: pointer/len come from `AxiommeOwnedBytes::from_vec`, which uses `Box<[u8]>`.
    unsafe {
        let slice_ptr = ptr::slice_from_raw_parts_mut(bytes.ptr, bytes.len);
        drop(Box::from_raw(slice_ptr));
    }
}

fn parse_required_c_string(
    raw: *const c_char,
    operation: &'static str,
    field: &'static str,
) -> std::result::Result<String, AxiommeFfiResult> {
    if raw.is_null() {
        return Err(AxiommeFfiResult::invalid_argument(
            operation,
            format!("{field} pointer is null"),
        ));
    }

    // SAFETY: `raw` is checked for null and expected to be a NUL-terminated C string.
    let c_str = unsafe { CStr::from_ptr(raw) };
    let value = c_str.to_str().map(str::trim).map_err(|err| {
        AxiommeFfiResult::invalid_argument(operation, format!("{field} must be valid UTF-8: {err}"))
    })?;
    if value.is_empty() {
        return Err(AxiommeFfiResult::invalid_argument(
            operation,
            format!("{field} must be non-empty"),
        ));
    }
    Ok(value.to_string())
}

fn parse_c_string_allow_empty(
    raw: *const c_char,
    operation: &'static str,
    field: &'static str,
) -> std::result::Result<String, AxiommeFfiResult> {
    if raw.is_null() {
        return Err(AxiommeFfiResult::invalid_argument(
            operation,
            format!("{field} pointer is null"),
        ));
    }

    // SAFETY: `raw` is checked for null and expected to be a NUL-terminated C string.
    let c_str = unsafe { CStr::from_ptr(raw) };
    c_str.to_str().map(str::to_string).map_err(|err| {
        AxiommeFfiResult::invalid_argument(operation, format!("{field} must be valid UTF-8: {err}"))
    })
}

fn parse_optional_c_string(
    raw: *const c_char,
    operation: &'static str,
    field: &'static str,
) -> std::result::Result<Option<String>, AxiommeFfiResult> {
    if raw.is_null() {
        return Ok(None);
    }

    // SAFETY: `raw` is non-null and expected to be a NUL-terminated C string.
    let c_str = unsafe { CStr::from_ptr(raw) };
    let value = c_str.to_str().map(str::trim).map_err(|err| {
        AxiommeFfiResult::invalid_argument(operation, format!("{field} must be valid UTF-8: {err}"))
    })?;
    if value.is_empty() {
        return Ok(None);
    }
    Ok(Some(value.to_string()))
}

fn runtime_from_ptr<'a>(
    runtime: *mut AxiommeRuntime,
    operation: &'static str,
) -> std::result::Result<&'a mut AxiommeRuntime, AxiommeFfiResult> {
    if runtime.is_null() {
        return Err(AxiommeFfiResult::invalid_argument(
            operation,
            "runtime pointer is null",
        ));
    }
    // SAFETY: pointer null-check is performed above; caller owns lifecycle.
    Ok(unsafe { &mut *runtime })
}

fn json_success(operation: &'static str, payload: &impl Serialize) -> AxiommeFfiResult {
    match serde_json::to_vec(payload) {
        Ok(payload) => AxiommeFfiResult::ok_json_bytes(payload),
        Err(err) => AxiommeFfiResult::internal_error(
            operation,
            format!("json encode failed for {operation}: {err}"),
        ),
    }
}

fn save_markdown_with_create(
    runtime: &mut AxiommeRuntime,
    uri: &str,
    content: &str,
    expected_etag: Option<&str>,
) -> std::result::Result<axiomme_core::models::MarkdownSaveResult, AxiomError> {
    match runtime.app.save_markdown(uri, content, expected_etag) {
        Ok(saved) => Ok(saved),
        Err(AxiomError::NotFound(_)) if expected_etag.is_none() => {
            let parsed_uri = validate_markdown_create_target(uri)?;
            if !runtime.app.fs.exists(&parsed_uri) {
                runtime.app.fs.write_atomic(&parsed_uri, "", false)?;
            }
            runtime.app.save_markdown(uri, content, None)
        }
        Err(err) => Err(err),
    }
}

fn validate_markdown_create_target(uri: &str) -> std::result::Result<AxiomUri, AxiomError> {
    let parsed = AxiomUri::parse(uri)?;
    if !matches!(
        parsed.scope(),
        Scope::Resources | Scope::User | Scope::Agent | Scope::Session
    ) {
        return Err(AxiomError::PermissionDenied(format!(
            "markdown editor does not allow scope: {}",
            parsed.scope()
        )));
    }

    let name = parsed.last_segment().ok_or_else(|| {
        AxiomError::Validation(format!("markdown target must include a filename: {parsed}"))
    })?;
    if name == ".abstract.md" || name == ".overview.md" {
        return Err(AxiomError::PermissionDenied(format!(
            "markdown editor cannot modify generated tier file: {parsed}"
        )));
    }

    let ext = Path::new(name)
        .extension()
        .and_then(|segment| segment.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(ext.as_str(), "md" | "markdown") {
        return Err(AxiomError::Validation(format!(
            "markdown editor only supports .md/.markdown targets: {parsed}"
        )));
    }

    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_bytes_empty_is_null() {
        let bytes = AxiommeOwnedBytes::empty();
        assert!(bytes.ptr.is_null());
        assert_eq!(bytes.len, 0);
    }

    #[test]
    fn owned_bytes_round_trip() {
        let original = br#"{"ok":true}"#.to_vec();
        let bytes = AxiommeOwnedBytes::from_vec(original.clone());
        assert!(!bytes.ptr.is_null());
        assert_eq!(bytes.len, original.len());

        // SAFETY: pointer and length are from `from_vec` and valid until freed.
        let slice = unsafe { std::slice::from_raw_parts(bytes.ptr, bytes.len) };
        assert_eq!(slice, original.as_slice());
        // SAFETY: `bytes` originates from `AxiommeOwnedBytes::from_vec`.
        unsafe {
            axiomme_owned_bytes_free(bytes);
        }
    }
}
