//! The HorizontalPodAutoscaler list's columns (#156), as `kubectl get hpa`
//! shows them - what it scales, each metric's current value against its
//! target, its bounds - with Replicas as current against desired.

use super::{Cell, ColumnDef, KindColumns};
use crate::k8s::resource::hpa;

const fn column(id: &'static str, title: &'static str, width: f32) -> ColumnDef {
    ColumnDef { id, title, width }
}

pub(super) static HORIZONTAL_POD_AUTOSCALER: KindColumns = KindColumns {
    columns: &[
        column("reference", "Reference", 160.),
        column("targets", "Targets", 220.),
        column("min_pods", "Min", 50.),
        column("max_pods", "Max", 50.),
        column("replicas", "Replicas", 80.),
    ],
    cells: |object| {
        let Some(hpa) = hpa::summarize(object) else {
            return Vec::new();
        };
        vec![
            Cell::text(format!("{}/{}", hpa.target.kind, hpa.target.name)),
            Cell::text(hpa.targets()),
            Cell::Number(i64::from(hpa.min_replicas)),
            Cell::Number(i64::from(hpa.max_replicas)),
            match (hpa.current_replicas, hpa.desired_replicas) {
                (None, None) => Cell::Empty,
                (current, desired) => Cell::Ratio(
                    i64::from(current.unwrap_or(0)),
                    i64::from(desired.unwrap_or(0)),
                ),
            },
        ]
    },
};
