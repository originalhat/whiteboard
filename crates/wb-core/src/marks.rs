//! Reading a freehand stroke as a mark on the board: a loop circles things, a line joins two
//! nodes, and a scribble or a line across a node strikes it out. Tolerances scale with the stroke's
//! size and with the zoom it was drawn at, never fixed pixels, so sloppy finger strokes still read.

use crate::id::{ItemId, NodeId};
use crate::layout::{NODE_HEIGHT, NODE_WIDTH};
use crate::model::{Board, Point, Relation, StrokePoint, Target};

/// How far off a stroke may land, in screen pixels at the zoom it was drawn at.
const SCREEN_TOLERANCE: f64 = 20.0;
/// How near a loop's end must come back to its start, as a share of the loop's size.
const CLOSING_SHARE: f64 = 0.3;
/// A stroke this many times longer than its bounding box is a scribble.
const SCRIBBLE_RATIO: f64 = 2.5;

pub fn resolve(board: &Board, stroke: &[StrokePoint], zoom: f64) -> Vec<Target> {
    let points: Vec<Point> = stroke.iter().map(|p| Point::new(p.x, p.y)).collect();
    let (Some(&first), Some(&last)) = (points.first(), points.last()) else {
        return Vec::new();
    };
    let tolerance = SCREEN_TOLERANCE / zoom.max(0.1);
    let bounds = Rect::around(&points);

    if is_loop(&points, bounds.diagonal()) {
        let items = enclosed(board, &points);
        if !items.is_empty() {
            return vec![target(Relation::Encloses, items)];
        }
    }
    if let (Some(a), Some(b)) = (
        node_at(board, first, tolerance),
        node_at(board, last, tolerance),
    ) && a != b
    {
        return vec![target(Relation::Connects, vec![a.into(), b.into()])];
    }
    if let Some(node) = struck(board, &points, &bounds, first, last) {
        return vec![target(Relation::Strikes, vec![node.into()])];
    }
    Vec::new()
}

fn target(relation: Relation, items: Vec<ItemId>) -> Target {
    Target { relation, items }
}

fn is_loop(points: &[Point], size: f64) -> bool {
    if points.len() < 8 || size == 0.0 {
        return false;
    }
    let end = points[points.len() - 1];
    points[..points.len() / 4]
        .iter()
        .any(|p| distance(*p, end) <= CLOSING_SHARE * size)
}

/// Nodes whose centre falls inside the loop, then edges whose middle does unless both of their
/// ends were circled too.
fn enclosed(board: &Board, polygon: &[Point]) -> Vec<ItemId> {
    let inside: Vec<NodeId> = board
        .nodes
        .values()
        .filter(|n| contains(polygon, centre(n.at)))
        .map(|n| n.id)
        .collect();
    let edges = board.edges.values().filter(|e| {
        let (a, b) = (
            centre(board.nodes[&e.from].at),
            centre(board.nodes[&e.to].at),
        );
        let middle = Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        contains(polygon, middle) && !(inside.contains(&e.from) && inside.contains(&e.to))
    });
    inside
        .iter()
        .map(|&n| n.into())
        .chain(edges.map(|e| e.id.into()))
        .collect()
}

fn node_at(board: &Board, point: Point, tolerance: f64) -> Option<NodeId> {
    board
        .nodes
        .values()
        .filter(|n| Rect::node(n.at).grow(tolerance).contains(point))
        .min_by(|a, b| {
            let (da, db) = (distance(centre(a.at), point), distance(centre(b.at), point));
            da.total_cmp(&db)
        })
        .map(|n| n.id)
}

fn struck(
    board: &Board,
    points: &[Point],
    bounds: &Rect,
    first: Point,
    last: Point,
) -> Option<NodeId> {
    let middle = bounds.centre();
    let node = board
        .nodes
        .values()
        .find(|n| Rect::node(n.at).contains(middle))?;
    let rect = Rect::node(node.at);
    let crosses = !rect.contains(first) && !rect.contains(last);
    let scribbled = path_length(points) >= SCRIBBLE_RATIO * bounds.diagonal();
    (crosses || scribbled).then_some(node.id)
}

fn centre(at: Point) -> Point {
    Point::new(at.x + NODE_WIDTH / 2.0, at.y + NODE_HEIGHT / 2.0)
}

fn distance(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

fn path_length(points: &[Point]) -> f64 {
    points.windows(2).map(|w| distance(w[0], w[1])).sum()
}

/// Even-odd ray casting, treating the stroke as closed.
fn contains(polygon: &[Point], p: Point) -> bool {
    let mut inside = false;
    let mut j = polygon.len() - 1;
    for i in 0..polygon.len() {
        let (a, b) = (polygon[i], polygon[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

struct Rect {
    min: Point,
    max: Point,
}

impl Rect {
    fn around(points: &[Point]) -> Rect {
        let fold = |f: fn(f64, f64) -> f64, pick: fn(&Point) -> f64, start: f64| {
            points.iter().map(pick).fold(start, f)
        };
        Rect {
            min: Point::new(
                fold(f64::min, |p| p.x, f64::MAX),
                fold(f64::min, |p| p.y, f64::MAX),
            ),
            max: Point::new(
                fold(f64::max, |p| p.x, f64::MIN),
                fold(f64::max, |p| p.y, f64::MIN),
            ),
        }
    }

    fn node(at: Point) -> Rect {
        Rect {
            min: at,
            max: Point::new(at.x + NODE_WIDTH, at.y + NODE_HEIGHT),
        }
    }

    fn grow(&self, by: f64) -> Rect {
        Rect {
            min: Point::new(self.min.x - by, self.min.y - by),
            max: Point::new(self.max.x + by, self.max.y + by),
        }
    }

    fn contains(&self, p: Point) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    fn centre(&self) -> Point {
        Point::new(
            (self.min.x + self.max.x) / 2.0,
            (self.min.y + self.max.y) / 2.0,
        )
    }

    fn diagonal(&self) -> f64 {
        distance(self.min, self.max)
    }
}
