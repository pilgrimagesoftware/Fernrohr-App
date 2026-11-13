//! A custom resource list's columns from its CRD's `additionalPrinterColumns`
//! (#352) - what `kubectl get` shows for the kind, defined by whoever wrote the
//! CRD, so nothing here names any CRD. Built once when the list's watch starts
//! ([`super::super::watch`] fetches the CRD); a CRD that declares none, or that
//! the user can't read, leaves the list on the generic condition columns.
//!
//! This module owns turning a CRD's column definitions into columns and an
//! object into their cells. The table's layout of them is `table::list_model`'s.

use super::Cell;
use crate::k8s::resource::status_tone::ConditionStatus;
use jsonpath_rust::JsonPath as _;
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::{
    CustomResourceColumnDefinition, CustomResourceDefinition,
};
use kube::api::DynamicObject;
use serde_json::Value;

/// The JSONPaths of what the base columns already show; a printer column
/// reading one of them would only repeat it.
const BASE_PATHS: [&str; 3] = [
    ".metadata.name",
    ".metadata.namespace",
    ".metadata.creationTimestamp",
];

/// A CRD version's printer columns, in the CRD's order.
#[derive(Clone, Debug, PartialEq)]
pub struct PrinterColumns {
    columns: Vec<PrinterColumn>,
}

/// One printer column: its header, and where and as what its value is read.
#[derive(Clone, Debug, PartialEq)]
pub struct PrinterColumn {
    /// `crd:` and the column's name - the key a saved layout names it by,
    /// apart from the base columns' keys.
    pub id: String,
    pub title: String,
    /// Default width in logical pixels, by type.
    pub width: f32,
    /// The column's `jsonPath` as a JSONPath query (`$` prefixed).
    path: String,
    kind: ValueType,
}

/// A printer column's declared `type` (OpenAPI's), which decides its cell.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ValueType {
    Integer,
    Number,
    Boolean,
    Date,
    /// `string`, and any type this build doesn't know.
    String,
}

impl ValueType {
    fn parse(type_: &str) -> Self {
        match type_ {
            "integer" => Self::Integer,
            "number" => Self::Number,
            "boolean" => Self::Boolean,
            "date" => Self::Date,
            _ => Self::String,
        }
    }

    fn width(self) -> f32 {
        match self {
            Self::Integer | Self::Number | Self::Boolean | Self::Date => 90.,
            Self::String => 160.,
        }
    }
}

impl PrinterColumns {
    /// `crd`'s columns for `version`, as `kubectl get` shows them: priority 0
    /// only (the rest are `-o wide`), less any that repeat a base column.
    /// `None` when that leaves none, or the CRD doesn't serve `version`.
    pub fn from_crd(crd: &CustomResourceDefinition, version: &str) -> Option<Self> {
        let served = crd.spec.versions.iter().find(|v| v.name == version)?;
        let columns: Vec<PrinterColumn> = served
            .additional_printer_columns
            .iter()
            .flatten()
            .filter(|column| column.priority.unwrap_or(0) == 0)
            .filter(|column| !BASE_PATHS.contains(&column.json_path.trim()))
            .map(PrinterColumn::new)
            .collect();
        (!columns.is_empty()).then_some(Self { columns })
    }

    pub fn columns(&self) -> &[PrinterColumn] {
        &self.columns
    }

    /// `object`'s cells, one per column, in order.
    pub fn cells_for(&self, object: &DynamicObject) -> Vec<Cell> {
        let Ok(value) = serde_json::to_value(object) else {
            return vec![Cell::Empty; self.columns.len()];
        };
        self.columns
            .iter()
            .map(|column| column.cell(&value))
            .collect()
    }
}

impl PrinterColumn {
    fn new(definition: &CustomResourceColumnDefinition) -> Self {
        let kind = ValueType::parse(&definition.type_);
        let path = definition.json_path.trim();
        Self {
            id: format!("crd:{}", definition.name.to_lowercase()),
            title: definition.name.clone(),
            width: kind.width(),
            path: if path.starts_with('$') {
                path.to_string()
            } else if path.starts_with('.') || path.starts_with('[') {
                format!("${path}")
            } else {
                format!("$.{path}")
            },
            kind,
        }
    }

    /// The cell `object` (as JSON) shows here: empty when the path finds
    /// nothing or doesn't parse; several matches joined with commas.
    fn cell(&self, object: &Value) -> Cell {
        let Ok(found) = object.query(&self.path) else {
            return Cell::Empty;
        };
        match found.as_slice() {
            [] => Cell::Empty,
            [one] => self.typed(one),
            many => Cell::text(
                many.iter()
                    .map(|value| plain(value))
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        }
    }

    /// One value as this column's type reads it. A string of exactly
    /// `True`, `False` or `Unknown` is a condition status, drawn in its tone.
    fn typed(&self, value: &Value) -> Cell {
        match (self.kind, value) {
            (_, Value::Null) => Cell::Empty,
            (ValueType::Integer, Value::Number(number)) => number
                .as_i64()
                .map_or_else(|| Cell::text(number.to_string()), Cell::Number),
            (ValueType::Date, Value::String(text)) => text
                .parse()
                .map_or_else(|_| Cell::text(text.clone()), Cell::Age),
            (ValueType::String | ValueType::Boolean, Value::String(text))
                if matches!(text.as_str(), "True" | "False" | "Unknown") =>
            {
                Cell::Condition(ConditionStatus::parse(Some(text)))
            }
            _ => Cell::text(plain(value)),
        }
    }
}

/// A JSON value as `kubectl` prints it: a string without its quotes,
/// anything else as JSON.
fn plain(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests;
