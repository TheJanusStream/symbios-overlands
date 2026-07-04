//! `--analyze-session` filters (B-5): subsystem / category / severity /
//! time-window narrowing parsed from the CLI flags.

use crate::diagnostics::event::{Category, SessionEvent, Severity, Subsystem};

use super::sections::CATEGORY_ORDER;

/// Filters for `--analyze-session`: restrict the report's *analysis* sections to
/// events matching a subsystem / category / minimum severity / time window. Every
/// field is optional; an unset field doesn't filter, and an all-`None` filter is
/// a no-op passthrough. The header (session identity) is never filtered — it
/// identifies the run — see [`super::report_with`]. Built from CLI strings by
/// [`Filters::parse`]; applied purely, so it unit-tests without file IO.
#[derive(Default, Clone, Debug)]
pub struct Filters {
    pub subsystem: Option<Subsystem>,
    pub category: Option<Category>,
    /// Minimum severity: an event matches if its severity is ≥ this.
    pub min_severity: Option<Severity>,
    /// Inclusive lower bound on `t_mono_secs`.
    pub since: Option<f64>,
    /// Inclusive upper bound on `t_mono_secs`.
    pub until: Option<f64>,
}

pub(super) fn parse_subsystem(s: &str) -> Option<Subsystem> {
    match s.to_ascii_lowercase().as_str() {
        "loading" => Some(Subsystem::Loading),
        "network" | "net" => Some(Subsystem::Network),
        "offload" => Some(Subsystem::Offload),
        "runtime" => Some(Subsystem::Runtime),
        "session" => Some(Subsystem::Session),
        _ => None,
    }
}

pub(super) fn parse_severity(s: &str) -> Option<Severity> {
    match s.to_ascii_lowercase().as_str() {
        "trace" => Some(Severity::Trace),
        "info" => Some(Severity::Info),
        "warn" | "warning" => Some(Severity::Warn),
        "error" => Some(Severity::Error),
        "critical" | "crit" => Some(Severity::Critical),
        _ => None,
    }
}

pub(super) fn parse_category(s: &str) -> Option<Category> {
    let s = s.to_ascii_lowercase();
    CATEGORY_ORDER
        .iter()
        .copied()
        .find(|c| format!("{c:?}").to_ascii_lowercase() == s)
}

impl Filters {
    /// Whether any filter is set (all-`None` = passthrough).
    pub fn is_active(&self) -> bool {
        self.subsystem.is_some()
            || self.category.is_some()
            || self.min_severity.is_some()
            || self.since.is_some()
            || self.until.is_some()
    }

    /// Whether `e` passes every set filter.
    pub fn matches(&self, e: &SessionEvent) -> bool {
        if self.subsystem.is_some_and(|sub| e.subsystem != sub) {
            return false;
        }
        if self.category.is_some_and(|cat| e.category != cat) {
            return false;
        }
        if self.min_severity.is_some_and(|min| e.severity < min) {
            return false;
        }
        if self.since.is_some_and(|since| e.t_mono_secs < since) {
            return false;
        }
        if self.until.is_some_and(|until| e.t_mono_secs > until) {
            return false;
        }
        true
    }

    /// A human summary of the active filters for the `[Filter]` header line.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some(sub) = self.subsystem {
            parts.push(format!("subsystem={sub:?}"));
        }
        if let Some(cat) = self.category {
            parts.push(format!("category={cat:?}"));
        }
        if let Some(min) = self.min_severity {
            parts.push(format!("severity≥{min:?}"));
        }
        match (self.since, self.until) {
            (Some(a), Some(b)) => parts.push(format!("t∈[{a:.1}s, {b:.1}s]")),
            (Some(a), None) => parts.push(format!("t≥{a:.1}s")),
            (None, Some(b)) => parts.push(format!("t≤{b:.1}s")),
            (None, None) => {}
        }
        if parts.is_empty() {
            "none".to_string()
        } else {
            parts.join(", ")
        }
    }

    /// Parse CLI filter strings into a [`Filters`], returning a clear error for
    /// an unknown subsystem / category / severity name (case-insensitive).
    pub fn parse(
        subsystem: Option<&str>,
        category: Option<&str>,
        severity: Option<&str>,
        since: Option<f64>,
        until: Option<f64>,
    ) -> Result<Filters, String> {
        let subsystem = match subsystem {
            Some(s) => Some(parse_subsystem(s).ok_or_else(|| {
                format!("unknown subsystem {s:?} (loading|network|offload|runtime|session)")
            })?),
            None => None,
        };
        let category = match category {
            Some(s) => Some(
                parse_category(s)
                    .ok_or_else(|| format!("unknown category {s:?} (see docs/diagnostics.md)"))?,
            ),
            None => None,
        };
        let min_severity = match severity {
            Some(s) => Some(parse_severity(s).ok_or_else(|| {
                format!("unknown severity {s:?} (trace|info|warn|error|critical)")
            })?),
            None => None,
        };
        Ok(Filters {
            subsystem,
            category,
            min_severity,
            since,
            until,
        })
    }
}
