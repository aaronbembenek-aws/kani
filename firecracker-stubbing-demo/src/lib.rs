// Copyright Kani Contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// This code is based on Firecracker (<https://firecracker-microvm.github.io/>)

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    },
};

/// This struct represents the strongly typed equivalent of the json body
/// from vsock related requests.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VsockDeviceConfig {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    /// ID of the vsock device.
    pub vsock_id: Option<String>,
    /// A 32-bit Context Identifier (CID) used to identify the guest.
    pub guest_cid: u32,
    /// Path to local unix socket.
    pub uds_path: String,
}

/// This is the original function we want to prove properties about
/// (some lines commented out for purposes of demo)
fn parse_put_vsock(body: &Body) -> Result<ParsedRequest, Error> {
    //METRICS.put_api_requests.vsock_count.inc();
    let vsock_cfg = serde_json::from_slice::<VsockDeviceConfig>(body.raw()).map_err(|err| {
        //METRICS.put_api_requests.vsock_fails.inc();
        err
    })?;

    // Check for the presence of deprecated `vsock_id` field.
    let mut deprecation_message = None;
    if vsock_cfg.vsock_id.is_some() {
        // vsock_id field in request is deprecated.
        //METRICS.deprecated_api.deprecated_http_api_calls.inc();
        deprecation_message = Some("PUT /vsock: vsock_id field is deprecated.");
    }

    // Construct the `ParsedRequest` object.
    let mut parsed_req = ParsedRequest::new_sync(VmmAction::SetVsockDevice(vsock_cfg));
    // If `vsock_id` was present, set the deprecation message in `parsing_info`.
    if let Some(msg) = deprecation_message {
        parsed_req.parsing_info().append_deprecation_message(msg);
    }

    Ok(parsed_req)
}

/// Prove that if we successfully parse a virtual socket put request, the
/// virtual socket device config structure will have a non-`None` id field if
/// and only if we also generate a message about this deprecated field.
#[kani::proof]
#[kani::unwind(2)]
#[kani::stub(serde_json::from_slice, mock_deserialize)]
fn demo_harness() {
    let body: Vec<u8> = vec![]; // raw data
    if let Ok(res) = parse_put_vsock(&Body::new(body)) {
        let (action, mut parsing_info) = res.into_parts();
        let config = get_vsock_device_config(action).unwrap();
        assert_eq!(config.vsock_id.is_some(), parsing_info.take_deprecation_message().is_some());
        // These two assertions are just to demonstrate that both of these
        // situations are actually covered by the proof.
        //assert!(config.vsock_id.is_none());
        //assert!(config.vsock_id.is_some());
    }
}

fn mock_deserialize<S, T>(_data: &[u8]) -> serde_json::Result<T>
where
    T: kani::Arbitrary,
{
    Ok(kani::any())
}

impl kani::Arbitrary for VsockDeviceConfig {
    fn any() -> Self {
        // Constrain the length of strings we consider. If you increase this,
        // you also need to increase the unwinding bound for the harness.
        const STR_LEN: usize = 1;
        let vsock_id = if kani::any() { None } else { Some(symbolic_string(STR_LEN)) };
        let guest_cid = kani::any();
        let uds_path = symbolic_string(STR_LEN);
        VsockDeviceConfig { vsock_id, guest_cid, uds_path }
    }
}

/// Create a string of the given length consisting of symbolic bytes
fn symbolic_string(len: usize) -> String {
    let mut v: Vec<u8> = Vec::with_capacity(len);
    for _ in 0..len {
        v.push(kani::any());
    }
    unsafe { String::from_utf8_unchecked(v) }
}

/// Helper function for harness
fn get_vsock_device_config(action: RequestAction) -> Option<VsockDeviceConfig> {
    if let RequestAction::Sync(vmm_action) = action {
        if let VmmAction::SetVsockDevice(dev) = *vmm_action {
            return Some(dev);
        }
    }
    return None;
}

// NECESSARY TYPES ETC. FROM FIRECRACKER //////////////////////////////////////

/// The Body associated with an HTTP Request or Response.
///
/// ## Examples
/// ```
/// use micro_http::Body;
/// let body = Body::new("This is a test body.".to_string());
/// assert_eq!(body.raw(), b"This is a test body.");
/// assert_eq!(body.len(), 20);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Body {
    /// Body of the HTTP message as bytes.
    pub body: Vec<u8>,
}

impl Body {
    /// Creates a new `Body` from a `String` input.
    pub fn new<T: Into<Vec<u8>>>(body: T) -> Self {
        Self { body: body.into() }
    }

    /// Returns the body as an `u8 slice`.
    pub fn raw(&self) -> &[u8] {
        self.body.as_slice()
    }

    /// Returns the length of the `Body`.
    pub fn len(&self) -> usize {
        self.body.len()
    }

    /// Checks if the body is empty, ie with zero length
    pub fn is_empty(&self) -> bool {
        self.body.len() == 0
    }
}

/// This enum represents the public interface of the VMM. Each action contains various
/// bits of information (ids, paths, etc.).
#[derive(PartialEq)]
pub enum VmmAction {
    /// Set the vsock device or update the one that already exists using the
    /// `VsockDeviceConfig` as input. This action can only be called before the microVM has
    /// booted.
    SetVsockDevice(VsockDeviceConfig),
    /// Get the machine configuration of the microVM.
    GetVmMachineConfig,
    /// Configure the logger using as input the `LoggerConfig`. This action can only be called
    /// before the microVM has booted.
    ConfigureLogger(LoggerConfig),
}

/// Enum used for setting the log level.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum LoggerLevel {
    /// When the level is set to `Error`, the logger will only contain entries
    /// that come from the `error` macro.
    Error,
    /// When the level is set to `Warning`, the logger will only contain entries
    /// that come from the `error` and `warn` macros.
    Warning,
    /// When the level is set to `Info`, the logger will only contain entries
    /// that come from the `error`, `warn` and `info` macros.
    Info,
    /// The most verbose log level.
    Debug,
}

/// Strongly typed structure used to describe the logger.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LoggerConfig {
    /// Named pipe or file used as output for logs.
    pub log_path: PathBuf,
    /// The level of the Logger.
    pub level: LoggerLevel,
    /// When enabled, the logger will append to the output the severity of the log entry.
    #[serde(default)]
    pub show_level: bool,
    /// When enabled, the logger will append the origin of the log entry.
    #[serde(default)]
    pub show_log_origin: bool,
}

enum RequestAction {
    Sync(Box<VmmAction>),
    ShutdownInternal, // !!! not an API, used by shutdown to thread::join the API thread
}

#[derive(Default)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct ParsingInfo {
    deprecation_message: Option<String>,
}

impl ParsingInfo {
    pub fn append_deprecation_message(&mut self, message: &str) {
        match self.deprecation_message.as_mut() {
            None => self.deprecation_message = Some(message.to_owned()),
            Some(s) => (*s).push_str(message),
        }
    }

    pub fn take_deprecation_message(&mut self) -> Option<String> {
        self.deprecation_message.take()
    }
}

struct ParsedRequest {
    action: RequestAction,
    parsing_info: ParsingInfo,
}

impl ParsedRequest {
    fn new(action: RequestAction) -> Self {
        Self { action, parsing_info: Default::default() }
    }

    /// Helper function to avoid boiler-plate code.
    pub(crate) fn new_sync(vmm_action: VmmAction) -> ParsedRequest {
        ParsedRequest::new(RequestAction::Sync(Box::new(vmm_action)))
    }

    fn parsing_info(&mut self) -> &mut ParsingInfo {
        &mut self.parsing_info
    }

    fn into_parts(self) -> (RequestAction, ParsingInfo) {
        (self.action, self.parsing_info)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusCode {
    /// 100, Continue
    Continue,
    /// 200, OK
    OK,
    /// 204, No Content
    NoContent,
    /// 400, Bad Request
    BadRequest,
    /// 401, Unauthorized
    Unauthorized,
    /// 404, Not Found
    NotFound,
    /// 405, Method Not Allowed
    MethodNotAllowed,
    /// 413, Payload Too Large
    PayloadTooLarge,
    /// 500, Internal Server Error
    InternalServerError,
    /// 501, Not Implemented
    NotImplemented,
    /// 503, Service Unavailable
    ServiceUnavailable,
}

#[derive(Debug, derive_more::From)]
enum Error {
    // A generic error, with a given status code and message to be turned into a fault message.
    Generic(StatusCode, String),
    // An error occurred when deserializing the json body of a request.
    SerdeJson(serde_json::Error),
}

// Static instance used for handling metrics.
lazy_static! {
    static ref METRICS: Metrics = Metrics::new();
}

/// Metrics system.
// All member fields have types which are Sync, and exhibit interior mutability, so
// we can call operations on metrics using a non-mut static global variable.
pub struct Metrics {
    // Metrics will get flushed here.
    metrics_buf: Mutex<Option<Box<dyn Write + Send>>>,
    is_initialized: AtomicBool,
    pub deprecated_api: DeprecatedApiMetrics,
    pub put_api_requests: PutRequestsMetrics,
}

impl Metrics {
    pub fn new() -> Self {
        Metrics {
            metrics_buf: Mutex::new(None),
            is_initialized: AtomicBool::new(false),
            deprecated_api: DeprecatedApiMetrics::default(),
            put_api_requests: PutRequestsMetrics::default(),
        }
    }
}

/// Used for defining new types of metrics that act as a counter (i.e they are continuously updated
/// by incrementing their value).
pub trait IncMetric {
    /// Adds `value` to the current counter.
    fn add(&self, value: usize);
    /// Increments by 1 unit the current counter.
    #[inline(never)]
    fn inc(&self) {
        self.add(1);
    }
    /// Returns current value of the counter.
    fn count(&self) -> usize;
}

/// Representation of a metric that is expected to be incremented from more than one thread, so more
/// synchronization is necessary.
// It's currently used for vCPU metrics. An alternative here would be
// to have one instance of every metric for each thread, and to
// aggregate them when writing. However this probably overkill unless we have a lot of vCPUs
// incrementing metrics very often. Still, it's there if we ever need it :-s
// We will be keeping two values for each metric for being able to reset
// counters on each metric.
// 1st member - current value being updated
// 2nd member - old value that gets the current value whenever metrics is flushed to disk
#[derive(Default)]
pub struct SharedIncMetric(AtomicUsize, AtomicUsize);

impl IncMetric for SharedIncMetric {
    // While the order specified for this operation is still Relaxed, the actual instruction will
    // be an asm "LOCK; something" and thus atomic across multiple threads, simply because of the
    // fetch_and_add (as opposed to "store(load() + 1)") implementation for atomics.
    // TODO: would a stronger ordering make a difference here?
    fn add(&self, value: usize) {
        self.0.fetch_add(value, Ordering::Relaxed);
    }

    fn count(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}

/// Metrics related to deprecated user-facing API calls.
#[derive(Default)]
pub struct DeprecatedApiMetrics {
    /// Total number of calls to deprecated HTTP endpoints.
    pub deprecated_http_api_calls: SharedIncMetric,
    /// Total number of calls to deprecated CMD line parameters.
    pub deprecated_cmd_line_api_calls: SharedIncMetric,
}

/// Metrics specific to PUT API Requests for counting user triggered actions and/or failures.
#[derive(Default)]
pub struct PutRequestsMetrics {
    /// Number of PUTs triggering an action on the VM.
    pub actions_count: SharedIncMetric,
    /// Number of failures in triggering an action on the VM.
    pub actions_fails: SharedIncMetric,
    /// Number of PUTs for attaching source of boot.
    pub boot_source_count: SharedIncMetric,
    /// Number of failures during attaching source of boot.
    pub boot_source_fails: SharedIncMetric,
    /// Number of PUTs triggering a block attach.
    pub drive_count: SharedIncMetric,
    /// Number of failures in attaching a block device.
    pub drive_fails: SharedIncMetric,
    /// Number of PUTs for initializing the logging system.
    pub logger_count: SharedIncMetric,
    /// Number of failures in initializing the logging system.
    pub logger_fails: SharedIncMetric,
    /// Number of PUTs for configuring the machine.
    pub machine_cfg_count: SharedIncMetric,
    /// Number of failures in configuring the machine.
    pub machine_cfg_fails: SharedIncMetric,
    /// Number of PUTs for initializing the metrics system.
    pub metrics_count: SharedIncMetric,
    /// Number of failures in initializing the metrics system.
    pub metrics_fails: SharedIncMetric,
    /// Number of PUTs for creating a new network interface.
    pub network_count: SharedIncMetric,
    /// Number of failures in creating a new network interface.
    pub network_fails: SharedIncMetric,
    /// Number of PUTs for creating mmds.
    pub mmds_count: SharedIncMetric,
    /// Number of failures in creating a new mmds.
    pub mmds_fails: SharedIncMetric,
    /// Number of PUTs for creating a vsock device.
    pub vsock_count: SharedIncMetric,
    /// Number of failures in creating a vsock device.
    pub vsock_fails: SharedIncMetric,
}
