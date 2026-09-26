//! Where things go, so the agent never places pixels.

use crate::model::Point;

pub const NODE_WIDTH: f64 = 200.0;
pub const NODE_HEIGHT: f64 = 80.0;
pub const PITCH_X: f64 = 340.0;
pub const PITCH_Y: f64 = 220.0;
const COLUMNS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Spot {
    Auto,
    Near(Point),
    /// Next to a point but off the straight lines through it, for things that annotate it.
    Aside(Point),
    Toward {
        from: Point,
        dx: f64,
        dy: f64,
    },
}

/// Two node positions are too close if their cells would touch.
pub fn overlaps(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < PITCH_X && (a.y - b.y).abs() < PITCH_Y
}

pub(crate) fn place(occupied: &[Point], spot: Spot) -> Point {
    let free = |p: &Point| !occupied.iter().any(|o| overlaps(*o, *p));
    match spot {
        Spot::Auto => (0..)
            .map(|i| {
                cell(
                    Point::new(0.0, 0.0),
                    (i % COLUMNS) as f64,
                    (i / COLUMNS) as f64,
                )
            })
            .find(free),
        Spot::Toward { from, dx, dy } => (1..)
            .map(|k| cell(from, dx * k as f64, dy * k as f64))
            .find(free),
        Spot::Near(anchor) => (1..)
            .flat_map(ring)
            .map(|(dx, dy)| cell(anchor, dx, dy))
            .find(free),
        Spot::Aside(anchor) => (1..)
            .flat_map(|r| {
                let (diagonal, straight): (Vec<_>, Vec<_>) = ring(r)
                    .into_iter()
                    .partition(|(dx, dy)| *dx != 0.0 && *dy != 0.0);
                diagonal.into_iter().chain(straight)
            })
            .map(|(dx, dy)| cell(anchor, dx, dy))
            .find(free),
    }
    .expect("the board is finite, so some cell is free")
}

fn cell(origin: Point, dx: f64, dy: f64) -> Point {
    Point::new(origin.x + dx * PITCH_X, origin.y + dy * PITCH_Y)
}

/// The cells `r` steps out from the centre, closest first, starting to the right and going clockwise.
fn ring(r: i32) -> Vec<(f64, f64)> {
    let mut cells: Vec<(i32, i32)> = (-r..=r)
        .flat_map(|dx| (-r..=r).map(move |dy| (dx, dy)))
        .filter(|(dx, dy)| dx.abs().max(dy.abs()) == r)
        .collect();
    cells.sort_by(|a, b| {
        let key = |&(dx, dy): &(i32, i32)| {
            let angle = (dy as f64)
                .atan2(dx as f64)
                .rem_euclid(std::f64::consts::TAU);
            (dx * dx + dy * dy, angle)
        };
        key(a).partial_cmp(&key(b)).unwrap()
    });
    cells
        .into_iter()
        .map(|(dx, dy)| (dx as f64, dy as f64))
        .collect()
}
