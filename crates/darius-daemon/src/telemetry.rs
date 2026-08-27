//! OpenTelemetry — span taxonomy and OTLP export.

use serde::{Deserialize, Serialize};

/// Span categories for the Darius daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpanCategory {
    Session,
    Subagent,
    ToolCall,
    ModelCall,
    Eval,
    Learn,
    Cache,
    A2A,
}

impl std::fmt::Display for SpanCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpanCategory::Session => write!(f, "session"),
            SpanCategory::Subagent => write!(f, "subagent"),
            SpanCategory::ToolCall => write!(f, "tool_call"),
            SpanCategory::ModelCall => write!(f, "model_call"),
            SpanCategory::Eval => write!(f, "eval"),
            SpanCategory::Learn => write!(f, "learn"),
            SpanCategory::Cache => write!(f, "cache"),
            SpanCategory::A2A => write!(f, "a2a"),
        }
    }
}

/// A telemetry span.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub name: String,
    pub category: SpanCategory,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub start_time: u64,
    pub end_time: Option<u64>,
    pub attributes: std::collections::HashMap<String, serde_json::Value>,
    pub status: SpanStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpanStatus {
    Ok,
    Error,
    InProgress,
}

/// OTLP exporter (stub).
pub struct OtlpExporter {
    endpoint: String,
    headers: std::collections::HashMap<String, String>,
}

impl OtlpExporter {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            headers: std::collections::HashMap::new(),
        }
    }

pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
    self.headers.insert(key.into().to_lowercase(), value.into());
    self
}

    /// Export spans to OTLP endpoint (stub).
    pub fn export(&self, spans: &[Span]) -> Result<(), String> {
        // Stub: in a real implementation, this would send spans via OTLP/gRPC.
        Ok(())
    }
}

/// Telemetry collector — collects and exports spans.
pub struct TelemetryCollector {
    spans: Vec<Span>,
    exporter: Option<OtlpExporter>,
}

impl TelemetryCollector {
    pub fn new() -> Self {
        Self {
            spans: Vec::new(),
            exporter: None,
        }
    }

    pub fn with_exporter(mut self, exporter: OtlpExporter) -> Self {
        self.exporter = Some(exporter);
        self
    }

    /// Start a new span.
    pub fn start_span(&mut self, name: impl Into<String>, category: SpanCategory) -> SpanHandle {
        let span = Span {
            name: name.into(),
            category,
            trace_id: uuid::Uuid::new_v4().to_string(),
            span_id: uuid::Uuid::new_v4().to_string(),
            parent_span_id: None,
            start_time: current_timestamp(),
            end_time: None,
            attributes: std::collections::HashMap::new(),
            status: SpanStatus::InProgress,
        };
        self.spans.push(span.clone());
        SpanHandle {
            span_id: span.span_id,
            collector: self,
        }
    }

    /// Get all collected spans.
    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// Export all collected spans.
    pub fn export(&self) -> Result<(), String> {
        match &self.exporter {
            Some(exporter) => exporter.export(&self.spans),
            None => Err("no exporter configured".into()),
        }
    }
}

/// Handle to an in-progress span.
pub struct SpanHandle<'a> {
    span_id: String,
    collector: &'a mut TelemetryCollector,
}

impl SpanHandle<'_> {
    /// Set an attribute on the span.
    pub fn set_attribute(&mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) {
        if let Some(span) = self.collector.spans.iter_mut().find(|s| s.span_id == self.span_id) {
            span.attributes.insert(key.into(), value.into());
        }
    }

    /// Mark the span as completed.
    pub fn end(self) {
        if let Some(span) = self.collector.spans.iter_mut().find(|s| s.span_id == self.span_id) {
            span.end_time = Some(current_timestamp());
            span.status = SpanStatus::Ok;
        }
    }

    /// Mark the span as failed.
    pub fn end_with_error(self, _error: &str) {
        if let Some(span) = self.collector.spans.iter_mut().find(|s| s.span_id == self.span_id) {
            span.end_time = Some(current_timestamp());
            span.status = SpanStatus::Error;
        }
    }
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_category_display() {
        assert_eq!(SpanCategory::Session.to_string(), "session");
        assert_eq!(SpanCategory::ToolCall.to_string(), "tool_call");
    }

    #[test]
    fn collector_starts_and_ends_span() {
        let mut collector = TelemetryCollector::new();
        let mut handle = collector.start_span("test_span", SpanCategory::Session);
        handle.set_attribute("key", "value");
        handle.end();

        let spans = collector.spans();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "test_span");
        assert_eq!(spans[0].status, SpanStatus::Ok);
        assert!(spans[0].end_time.is_some());
    }

    #[test]
    fn span_handle_end_with_error() {
        let mut collector = TelemetryCollector::new();
        let handle = collector.start_span("failing_span", SpanCategory::ToolCall);
        handle.end_with_error("something went wrong");

        let spans = collector.spans();
        assert_eq!(spans[0].status, SpanStatus::Error);
    }

    #[test]
    fn otlp_exporter_with_headers() {
        let exporter = OtlpExporter::new("http://localhost:4317")
            .with_header("Authorization", "Bearer token");
        assert_eq!(exporter.endpoint, "http://localhost:4317");
        assert_eq!(exporter.headers.get("authorization").unwrap(), "Bearer token");
    }

    #[test]
    fn telemetry_collector_export() {
        let exporter = OtlpExporter::new("http://localhost:4317");
        let collector = TelemetryCollector::new().with_exporter(exporter);
        assert!(collector.export().is_ok());
    }
}
