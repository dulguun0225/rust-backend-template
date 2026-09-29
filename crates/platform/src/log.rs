//! The one typed logging facade, and the only user of the `tracing` event and span macros (`clippy.toml`
//! bans them in every other crate).
//!
//! - WARN and above name a [`LogEvent`] catalog constant, so alert rules and searches target a stable key.
//! - INFO and DEBUG carry a `&'static str` message, never a formatted one.
//! - Structured values are typed [`LogField`]s: an id, a count, a flag or a catalog code. There is no
//!   string-valued field, so a name, an email or an account number cannot be logged by construction.
//! - Every event carries `module` (the emitting module's path) and, inside a request, the request span's
//!   `correlation_id`, which a 500's `incidentId` repeats.

use std::io;
use std::sync::{Arc, Mutex, PoisonError};

use tracing_subscriber::fmt::MakeWriter;
use uuid::Uuid;

/// A catalog code a [`LogField::code`] may carry: the wire string of a `wire_errors!` or `field_codes!` enum.
pub trait Code {
    /// The wire string.
    fn code(&self) -> &'static str;
}

/// The WARN-and-above event catalog. The wire string is immutable once shipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogEvent {
    /// A request ended in an unexpected failure and became a coded 500.
    RequestUnhandledError,
    /// A panic outside any request reached the process's panic hook.
    ProcessPanic,
}

impl LogEvent {
    /// The stable catalog key.
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            Self::RequestUnhandledError => "request.unhandled-error",
            Self::ProcessPanic => "process.panic",
        }
    }

    const fn level(self) -> Severity {
        match self {
            Self::RequestUnhandledError | Self::ProcessPanic => Severity::Error,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Severity {
    Error,
}

/// A typed structured field. Keys are static; values are never free text.
#[derive(Debug, Clone, Copy)]
pub struct LogField {
    key: &'static str,
    value: FieldValue,
}

#[derive(Debug, Clone, Copy)]
enum FieldValue {
    Id(Uuid),
    Count(u64),
    Flag(bool),
    Code(&'static str),
}

impl LogField {
    /// An entity id: the safe way to reference a record.
    #[must_use]
    pub const fn id(key: &'static str, id: Uuid) -> Self {
        Self { key, value: FieldValue::Id(id) }
    }

    /// A count or a size.
    #[must_use]
    pub const fn count(key: &'static str, n: u64) -> Self {
        Self { key, value: FieldValue::Count(n) }
    }

    /// A flag.
    #[must_use]
    pub const fn flag(key: &'static str, b: bool) -> Self {
        Self { key, value: FieldValue::Flag(b) }
    }

    /// A catalog code, by its wire string, never free text.
    #[must_use]
    pub fn code(key: &'static str, code: &impl Code) -> Self {
        Self { key, value: FieldValue::Code(code.code()) }
    }
}

/// The facade. One per module: `static LOG: Log = Log::new(module_path!());`.
#[derive(Debug, Clone, Copy)]
pub struct Log {
    module: &'static str,
}

impl Log {
    /// A facade for the module whose path is given, normally `module_path!()`.
    #[must_use]
    pub const fn new(module: &'static str) -> Self {
        Self { module }
    }

    /// A catalog event at the catalog's level.
    pub fn event(&self, event: LogEvent, fields: &[LogField]) {
        let rendered = render(fields);
        match event.level() {
            Severity::Error => {
                tracing::error!(module = self.module, event = event.wire(), fields = %rendered, "{}", event.wire())
            }
        }
    }

    /// A catalog event with the failure that caused it, rendered by `Display` into the server log only.
    pub fn event_with_cause(&self, event: LogEvent, cause: &str, fields: &[LogField]) {
        let rendered = render(fields);
        match event.level() {
            Severity::Error => {
                tracing::error!(module = self.module, event = event.wire(), cause = cause, fields = %rendered, "{}", event.wire());
            }
        }
    }

    /// An INFO line with a static message.
    pub fn info(&self, message: &'static str, fields: &[LogField]) {
        let rendered = render(fields);
        tracing::info!(module = self.module, fields = %rendered, "{message}");
    }

    /// A DEBUG line with a static message.
    pub fn debug(&self, message: &'static str, fields: &[LogField]) {
        let rendered = render(fields);
        tracing::debug!(module = self.module, fields = %rendered, "{message}");
    }
}

fn render(fields: &[LogField]) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for field in fields {
        let value = match field.value {
            FieldValue::Id(id) => serde_json::Value::String(id.to_string()),
            FieldValue::Count(n) => serde_json::Value::from(n),
            FieldValue::Flag(b) => serde_json::Value::Bool(b),
            FieldValue::Code(c) => serde_json::Value::String(c.to_owned()),
        };
        map.insert(field.key.to_owned(), value);
    }
    serde_json::Value::Object(map)
}

/// The span every request runs inside; its `correlation_id` appears on every event the request emits.
#[must_use]
pub fn request_span(correlation_id: Uuid) -> tracing::Span {
    tracing::info_span!("request", correlation_id = %correlation_id)
}

/// The one subscriber shape: JSON lines, one object per event, the current span's fields under `span`.
/// The server installs it over stdout at INFO; tests install it over a [`Capture`].
pub fn json_subscriber<W>(writer: W, max_level: tracing::Level) -> impl tracing::Subscriber + Send + Sync
where
    W: for<'w> MakeWriter<'w> + Send + Sync + 'static,
{
    tracing_subscriber::fmt()
        .json()
        .with_current_span(true)
        .with_span_list(false)
        .with_target(false)
        .with_max_level(max_level)
        .with_writer(writer)
        .finish()
}

/// An in-memory log sink for tests: install `json_subscriber(capture.clone(), tracing::Level::DEBUG)` and read the lines back.
#[derive(Debug, Clone, Default)]
pub struct Capture(Arc<Mutex<Vec<u8>>>);

impl Capture {
    /// Every captured line, parsed.
    #[must_use]
    pub fn events(&self) -> Vec<serde_json::Value> {
        let bytes = self.0.lock().unwrap_or_else(PoisonError::into_inner).clone();
        String::from_utf8_lossy(&bytes).lines().filter_map(|line| line.parse().ok()).collect()
    }

    /// Everything captured, as text.
    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap_or_else(PoisonError::into_inner)).into_owned()
    }
}

impl io::Write for Capture {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'w> MakeWriter<'w> for Capture {
    type Writer = Self;

    fn make_writer(&'w self) -> Self::Writer {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{Capture, Log, LogEvent, LogField, json_subscriber, request_span};
    use crate::catalog::WireError as _;

    crate::wire_errors! {
        enum Codes {
            Gone = ("probe.gone", 410),
        }
    }

    static LOG: Log = Log::new(module_path!());

    #[test]
    fn a_scoped_event_carries_module_correlation_and_typed_fields() {
        let capture = Capture::default();
        let id = crate::ids::new_id();
        tracing::subscriber::with_default(json_subscriber(capture.clone(), tracing::Level::DEBUG), || {
            request_span(id).in_scope(|| {
                LOG.event(
                    LogEvent::RequestUnhandledError,
                    &[LogField::count("attempts", 3), LogField::code("code", &Codes::Gone)],
                );
            });
        });
        let events = capture.events();
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event["level"], "ERROR");
        assert_eq!(event["fields"]["event"], "request.unhandled-error");
        assert_eq!(event["fields"]["module"], "platform::log::tests");
        assert_eq!(event["span"]["correlation_id"], id.to_string());
        let fields: serde_json::Value = event["fields"]["fields"].as_str().unwrap().parse().unwrap();
        assert_eq!(fields["attempts"], 3);
        assert_eq!(fields["code"], Codes::Gone.wire());
    }

    #[test]
    fn an_event_outside_a_request_carries_no_span_and_still_emits() {
        let capture = Capture::default();
        tracing::subscriber::with_default(json_subscriber(capture.clone(), tracing::Level::DEBUG), || {
            LOG.info("started", &[LogField::flag("ready", true)]);
            LOG.debug("detail", &[]);
            LOG.event_with_cause(LogEvent::ProcessPanic, "cause text", &[]);
        });
        let events = capture.events();
        assert_eq!(events.len(), 3);
        assert!(events.iter().all(|e| e.get("span").is_none()));
        assert_eq!(events[0]["fields"]["message"], "started");
        assert_eq!(events[1]["level"], "DEBUG");
        assert_eq!(events[2]["fields"]["cause"], "cause text");
        assert_eq!(events[2]["fields"]["event"], "process.panic");
    }
}
