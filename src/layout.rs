//! Tabs and the tree of split panes inside each tab.

use egui::{pos2, Rect};

pub enum Node {
    Leaf(u64),
    /// `side_by_side`: the halves sit left/right (vertical divider).
    Split { side_by_side: bool, ratio: f32, a: Box<Node>, b: Box<Node> },
}

pub struct Divider {
    pub path: Vec<bool>,
    pub parent: Rect,
    pub line: Rect,
    pub side_by_side: bool,
}

impl Node {
    pub fn leaves(&self, out: &mut Vec<u64>) {
        match self {
            Node::Leaf(id) => out.push(*id),
            Node::Split { a, b, .. } => {
                a.leaves(out);
                b.leaves(out);
            }
        }
    }

    pub fn split(&mut self, target: u64, new: u64, side_by_side: bool) -> bool {
        match self {
            Node::Leaf(id) if *id == target => {
                *self = Node::Split { side_by_side, ratio: 0.5, a: Box::new(Node::Leaf(target)), b: Box::new(Node::Leaf(new)) };
                true
            }
            Node::Leaf(_) => false,
            Node::Split { a, b, .. } => a.split(target, new, side_by_side) || b.split(target, new, side_by_side),
        }
    }

    pub fn remove(self, target: u64) -> Option<Node> {
        match self {
            Node::Leaf(id) => (id != target).then_some(Node::Leaf(id)),
            Node::Split { side_by_side, ratio, a, b } => match (a.remove(target), b.remove(target)) {
                (Some(a), Some(b)) => Some(Node::Split { side_by_side, ratio, a: Box::new(a), b: Box::new(b) }),
                (Some(x), None) | (None, Some(x)) => Some(x),
                (None, None) => None,
            },
        }
    }

    pub fn collect(&self, rect: Rect, path: &mut Vec<bool>, leaves: &mut Vec<(u64, Rect)>, divs: &mut Vec<Divider>) {
        match self {
            Node::Leaf(id) => leaves.push((*id, rect)),
            Node::Split { side_by_side, ratio, a, b } => {
                let (ra, rb, line) = if *side_by_side {
                    let x = (rect.left() + rect.width() * ratio).round();
                    (
                        Rect::from_min_max(rect.min, pos2(x, rect.bottom())),
                        Rect::from_min_max(pos2(x + 1.0, rect.top()), rect.max),
                        Rect::from_min_max(pos2(x, rect.top()), pos2(x + 1.0, rect.bottom())),
                    )
                } else {
                    let y = (rect.top() + rect.height() * ratio).round();
                    (
                        Rect::from_min_max(rect.min, pos2(rect.right(), y)),
                        Rect::from_min_max(pos2(rect.left(), y + 1.0), rect.max),
                        Rect::from_min_max(pos2(rect.left(), y), pos2(rect.right(), y + 1.0)),
                    )
                };
                divs.push(Divider { path: path.clone(), parent: rect, line, side_by_side: *side_by_side });
                path.push(false);
                a.collect(ra, path, leaves, divs);
                path.pop();
                path.push(true);
                b.collect(rb, path, leaves, divs);
                path.pop();
            }
        }
    }

    pub fn set_ratio(&mut self, path: &[bool], r: f32) {
        if let Node::Split { ratio, a, b, .. } = self {
            match path.split_first() {
                None => *ratio = r,
                Some((false, rest)) => a.set_ratio(rest, r),
                Some((true, rest)) => b.set_ratio(rest, r),
            }
        }
    }
}

pub struct Tab {
    pub root: Node,
    pub active: u64,
    pub activity: bool,
    pub bell: bool,
}

impl Tab {
    pub fn leaves(&self) -> Vec<u64> {
        let mut v = Vec::new();
        self.root.leaves(&mut v);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_and_close() {
        let mut n = Node::Leaf(1);
        assert!(n.split(1, 2, true));
        assert!(n.split(2, 3, false));
        let mut l = Vec::new();
        n.leaves(&mut l);
        assert_eq!(l, vec![1, 2, 3]);
        let n = n.remove(2).unwrap();
        let mut l = Vec::new();
        n.leaves(&mut l);
        assert_eq!(l, vec![1, 3]);
        assert!(Node::Leaf(1).remove(1).is_none());
    }
}
