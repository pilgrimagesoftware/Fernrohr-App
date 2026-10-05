//! Arranging dock groups from the keyboard (`panel-move-keybindings`): which
//! group lies in a direction from another, and the split / move / merge / close
//! commands. The window (`util::shell::arrange`) runs them on the dock.
//!
//! "The group to the right" is geometric: each tab group's rectangle is laid
//! out from the pane tree's own split proportions (the rendered layout, scaled
//! to a unit square), and the adjacent group is the nearest one lying entirely
//! on that side and overlapping the focused group across the other axis. When
//! two are equally near and equally overlapping - one beside two stacked panes
//! - the choice is ambiguous and there is none, rather than a guess.

use gpui_kit::Axis;
use gpui_kit::component::dock::{NodeId, PaneNode, PaneRef};

/// A direction on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
// UNWIRED(#122): the arrange commands' geometry, ahead of the commands;
// nothing outside tests calls it yet.
#[allow(dead_code)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// A group's rectangle in the unit square the dock region is scaled to.
#[derive(Clone, Copy, Debug, PartialEq)]
// UNWIRED(#122): the arrange commands' geometry, ahead of the commands;
// nothing outside tests calls it yet.
#[allow(dead_code)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// Every tab group under `root` and its rectangle, splits divided by their
/// sizes: a measured size weighs as itself, an unmeasured one as the mean of
/// its measured siblings - or all equally when none is measured, as the dock
/// lays out a fresh split.
// UNWIRED(#122): the arrange commands' geometry, ahead of the commands;
// nothing outside tests calls it yet.
#[allow(dead_code)]
pub fn group_rects(root: &PaneNode) -> Vec<(NodeId, Rect)> {
    let mut rects = Vec::new();
    lay_out(
        root,
        Rect {
            left: 0.,
            top: 0.,
            right: 1.,
            bottom: 1.,
        },
        &mut rects,
    );
    rects
}

fn lay_out(node: &PaneNode, rect: Rect, rects: &mut Vec<(NodeId, Rect)>) {
    match node.kind() {
        PaneRef::Tabs { .. } => rects.push((node.id(), rect)),
        PaneRef::Split {
            axis,
            children,
            sizes,
        } => {
            let measured: Vec<f32> = sizes
                .iter()
                .flatten()
                .map(|size| f32::from(*size))
                .collect();
            let fallback = if measured.is_empty() {
                1.
            } else {
                measured.iter().sum::<f32>() / measured.len() as f32
            };
            let weights: Vec<f32> = (0..children.len())
                .map(|ix| {
                    sizes
                        .get(ix)
                        .copied()
                        .flatten()
                        .map_or(fallback, f32::from)
                        .max(f32::EPSILON)
                })
                .collect();
            let total: f32 = weights.iter().sum();
            let mut start = 0.;
            for (child, weight) in children.iter().zip(weights) {
                let end = start + weight / total;
                let child_rect = match axis {
                    Axis::Horizontal => Rect {
                        left: rect.left + (rect.right - rect.left) * start,
                        right: rect.left + (rect.right - rect.left) * end,
                        ..rect
                    },
                    Axis::Vertical => Rect {
                        top: rect.top + (rect.bottom - rect.top) * start,
                        bottom: rect.top + (rect.bottom - rect.top) * end,
                        ..rect
                    },
                };
                lay_out(child, child_rect, rects);
                start = end;
            }
        }
    }
}

const EPSILON: f32 = 1e-4;

/// The group adjacent to `from` in `direction` among `rects`, or `None` when
/// none lies that way or two tie for it.
// UNWIRED(#122): the arrange commands' geometry, ahead of the commands;
// nothing outside tests calls it yet.
#[allow(dead_code)]
pub fn adjacent(rects: &[(NodeId, Rect)], from: NodeId, direction: Direction) -> Option<NodeId> {
    let (_, origin) = rects.iter().find(|(id, _)| *id == from)?;
    let overlap = |a0: f32, a1: f32, b0: f32, b1: f32| (a1.min(b1) - a0.max(b0)).max(0.);
    let candidates: Vec<(NodeId, f32, f32)> = rects
        .iter()
        .filter(|(id, _)| *id != from)
        .filter_map(|(id, rect)| {
            let (gap, shared) = match direction {
                Direction::Right => (
                    rect.left - origin.right,
                    overlap(origin.top, origin.bottom, rect.top, rect.bottom),
                ),
                Direction::Left => (
                    origin.left - rect.right,
                    overlap(origin.top, origin.bottom, rect.top, rect.bottom),
                ),
                Direction::Down => (
                    rect.top - origin.bottom,
                    overlap(origin.left, origin.right, rect.left, rect.right),
                ),
                Direction::Up => (
                    origin.top - rect.bottom,
                    overlap(origin.left, origin.right, rect.left, rect.right),
                ),
            };
            (gap > -EPSILON && shared > EPSILON).then_some((*id, gap, shared))
        })
        .collect();
    let nearest = candidates
        .iter()
        .map(|(_, gap, _)| *gap)
        .fold(f32::INFINITY, f32::min);
    let closest: Vec<&(NodeId, f32, f32)> = candidates
        .iter()
        .filter(|(_, gap, _)| (gap - nearest).abs() < EPSILON)
        .collect();
    let widest = closest
        .iter()
        .map(|(_, _, shared)| *shared)
        .fold(0., f32::max);
    let mut best = closest
        .into_iter()
        .filter(|(_, _, shared)| (shared - widest).abs() < EPSILON);
    let (id, _, _) = best.next()?;
    best.next().is_none().then_some(*id)
}

#[cfg(test)]
mod tests;
