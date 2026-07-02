//! Binary tree split pane layout engine.
//!
//! The backend owns all layout mutations. The frontend only renders the tree
//! it receives from commands, so the invariants below can never be violated
//! by UI races:
//!   - a Split node always has exactly two children (guaranteed by the type)
//!   - closing a pane promotes its sibling into the parent's position,
//!     so unary splits can never exist
//!   - ratio is always inside [0.1, 0.9]
//!   - node ids are unique within a tree
//!   - a tree always contains at least one pane (closing the last pane is refused)

use crate::model::{new_id, Direction, PaneLeaf, PaneNode, SplitNode};
use std::collections::HashSet;

pub const RATIO_MIN: f32 = 0.1;
pub const RATIO_MAX: f32 = 0.9;
pub const DEFAULT_RATIO: f32 = 0.5;

#[derive(Debug, PartialEq, Eq)]
pub enum LayoutError {
    PaneNotFound(String),
    SplitNotFound(String),
    /// Closing the last pane of a workspace is refused (documented policy).
    LastPane,
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayoutError::PaneNotFound(id) => write!(f, "pane not found: {id}"),
            LayoutError::SplitNotFound(id) => write!(f, "split not found: {id}"),
            LayoutError::LastPane => write!(f, "cannot close the last pane in a workspace"),
        }
    }
}

pub fn clamp_ratio(ratio: f32) -> f32 {
    if !ratio.is_finite() {
        return DEFAULT_RATIO;
    }
    ratio.clamp(RATIO_MIN, RATIO_MAX)
}

pub fn new_pane_leaf(cwd: &str) -> PaneLeaf {
    PaneLeaf {
        id: new_id(),
        session_id: None,
        cwd: cwd.to_string(),
        command: None,
        labels: Vec::new(),
        allow_injection: false,
        allow_observe: false,
        startup_command: None,
    }
}

fn placeholder() -> PaneNode {
    PaneNode::Pane(PaneLeaf {
        id: String::new(),
        session_id: None,
        cwd: String::new(),
        command: None,
        labels: Vec::new(),
        allow_injection: false,
        allow_observe: false,
        startup_command: None,
    })
}

/// Replace the target pane with a split node: the existing pane becomes
/// `first`, the new pane becomes `second`, ratio starts at 0.5.
/// Returns the id of the newly created pane.
pub fn split_pane(
    root: &mut PaneNode,
    target_pane_id: &str,
    direction: Direction,
    new_leaf: PaneLeaf,
) -> Result<String, LayoutError> {
    let new_id_ret = new_leaf.id.clone();
    if try_split(root, target_pane_id, direction, &mut Some(new_leaf)) {
        Ok(new_id_ret)
    } else {
        Err(LayoutError::PaneNotFound(target_pane_id.to_string()))
    }
}

fn try_split(
    node: &mut PaneNode,
    target: &str,
    direction: Direction,
    new_leaf: &mut Option<PaneLeaf>,
) -> bool {
    match node {
        PaneNode::Pane(leaf) if leaf.id == target => {
            let old = std::mem::replace(node, placeholder());
            let leaf = new_leaf.take().expect("new leaf consumed twice");
            *node = PaneNode::Split(SplitNode {
                id: new_id(),
                direction,
                ratio: DEFAULT_RATIO,
                first: Box::new(old),
                second: Box::new(PaneNode::Pane(leaf)),
            });
            true
        }
        PaneNode::Pane(_) => false,
        PaneNode::Split(s) => {
            try_split(&mut s.first, target, direction, new_leaf)
                || try_split(&mut s.second, target, direction, new_leaf)
        }
    }
}

/// Remove the target pane and promote its sibling into the parent split's
/// position. Returns the removed leaf so the caller can terminate its PTY
/// session. Refuses to remove the last remaining pane.
pub fn close_pane(root: &mut PaneNode, target_pane_id: &str) -> Result<PaneLeaf, LayoutError> {
    if let PaneNode::Pane(leaf) = root {
        if leaf.id == target_pane_id {
            return Err(LayoutError::LastPane);
        }
    }
    try_close(root, target_pane_id)
        .ok_or_else(|| LayoutError::PaneNotFound(target_pane_id.to_string()))
}

fn try_close(node: &mut PaneNode, target: &str) -> Option<PaneLeaf> {
    let PaneNode::Split(s) = node else {
        return None;
    };
    let first_is = matches!(&*s.first, PaneNode::Pane(l) if l.id == target);
    let second_is = matches!(&*s.second, PaneNode::Pane(l) if l.id == target);
    if first_is || second_is {
        let removed_box = std::mem::replace(
            if first_is { &mut s.first } else { &mut s.second },
            Box::new(placeholder()),
        );
        let kept_box = std::mem::replace(
            if first_is { &mut s.second } else { &mut s.first },
            Box::new(placeholder()),
        );
        let removed = match *removed_box {
            PaneNode::Pane(l) => l,
            PaneNode::Split(_) => unreachable!("checked to be a pane above"),
        };
        *node = *kept_box;
        return Some(removed);
    }
    try_close(&mut s.first, target).or_else(|| try_close(&mut s.second, target))
}

/// Set the ratio of a split node. The ratio is clamped to [0.1, 0.9];
/// the clamped value is returned.
pub fn resize_split(root: &mut PaneNode, split_id: &str, ratio: f32) -> Result<f32, LayoutError> {
    let clamped = clamp_ratio(ratio);
    if set_ratio(root, split_id, clamped) {
        Ok(clamped)
    } else {
        Err(LayoutError::SplitNotFound(split_id.to_string()))
    }
}

fn set_ratio(node: &mut PaneNode, split_id: &str, ratio: f32) -> bool {
    match node {
        PaneNode::Pane(_) => false,
        PaneNode::Split(s) => {
            if s.id == split_id {
                s.ratio = ratio;
                true
            } else {
                set_ratio(&mut s.first, split_id, ratio) || set_ratio(&mut s.second, split_id, ratio)
            }
        }
    }
}

pub fn collect_panes(node: &PaneNode) -> Vec<&PaneLeaf> {
    let mut out = Vec::new();
    fn walk<'a>(n: &'a PaneNode, out: &mut Vec<&'a PaneLeaf>) {
        match n {
            PaneNode::Pane(l) => out.push(l),
            PaneNode::Split(s) => {
                walk(&s.first, out);
                walk(&s.second, out);
            }
        }
    }
    walk(node, &mut out);
    out
}

pub fn collect_panes_mut(node: &mut PaneNode) -> Vec<&mut PaneLeaf> {
    let mut out = Vec::new();
    fn walk<'a>(n: &'a mut PaneNode, out: &mut Vec<&'a mut PaneLeaf>) {
        match n {
            PaneNode::Pane(l) => out.push(l),
            PaneNode::Split(s) => {
                walk(&mut s.first, out);
                walk(&mut s.second, out);
            }
        }
    }
    walk(node, &mut out);
    out
}

pub fn find_pane<'a>(node: &'a PaneNode, pane_id: &str) -> Option<&'a PaneLeaf> {
    collect_panes(node).into_iter().find(|l| l.id == pane_id)
}

pub fn first_pane_id(node: &PaneNode) -> String {
    match node {
        PaneNode::Pane(l) => l.id.clone(),
        PaneNode::Split(s) => first_pane_id(&s.first),
    }
}

pub fn contains_pane(node: &PaneNode, pane_id: &str) -> bool {
    find_pane(node, pane_id).is_some()
}

/// Structural invariant check, used by tests and defensively on config load.
pub fn check_invariants(root: &PaneNode) -> Result<(), String> {
    let mut ids: HashSet<&str> = HashSet::new();
    let mut pane_count = 0usize;
    fn walk<'a>(
        n: &'a PaneNode,
        ids: &mut HashSet<&'a str>,
        pane_count: &mut usize,
    ) -> Result<(), String> {
        match n {
            PaneNode::Pane(l) => {
                if l.id.is_empty() {
                    return Err("pane with empty id".into());
                }
                if !ids.insert(&l.id) {
                    return Err(format!("duplicate node id: {}", l.id));
                }
                *pane_count += 1;
                Ok(())
            }
            PaneNode::Split(s) => {
                if s.id.is_empty() {
                    return Err("split with empty id".into());
                }
                if !ids.insert(&s.id) {
                    return Err(format!("duplicate node id: {}", s.id));
                }
                if !(RATIO_MIN..=RATIO_MAX).contains(&s.ratio) {
                    return Err(format!("split {} ratio {} out of range", s.id, s.ratio));
                }
                walk(&s.first, ids, pane_count)?;
                walk(&s.second, ids, pane_count)
            }
        }
    }
    walk(root, &mut ids, &mut pane_count)?;
    if pane_count == 0 {
        return Err("tree contains no panes".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(id: &str) -> PaneNode {
        PaneNode::Pane(PaneLeaf {
            id: id.into(),
            session_id: None,
            cwd: "C:/".into(),
            command: None,
            labels: Vec::new(),
            allow_injection: false,
            allow_observe: false,
            startup_command: None,
        })
    }

    fn single_root() -> PaneNode {
        leaf("p1")
    }

    #[test]
    fn split_replaces_pane_with_split_node() {
        let mut root = single_root();
        let new_id =
            split_pane(&mut root, "p1", Direction::Row, new_pane_leaf("C:/")).expect("split ok");
        match &root {
            PaneNode::Split(s) => {
                assert_eq!(s.direction, Direction::Row);
                assert!((s.ratio - 0.5).abs() < f32::EPSILON);
                assert!(matches!(&*s.first, PaneNode::Pane(l) if l.id == "p1"));
                assert!(matches!(&*s.second, PaneNode::Pane(l) if l.id == new_id));
            }
            _ => panic!("root must be a split"),
        }
        check_invariants(&root).unwrap();
    }

    #[test]
    fn split_nested_target() {
        let mut root = single_root();
        split_pane(&mut root, "p1", Direction::Row, new_pane_leaf("C:/")).unwrap();
        let second_id = split_pane(&mut root, "p1", Direction::Column, new_pane_leaf("C:/")).unwrap();
        // root(row) -> [ split(column)->[p1, second_id], newer ]
        match &root {
            PaneNode::Split(s) => match &*s.first {
                PaneNode::Split(inner) => {
                    assert_eq!(inner.direction, Direction::Column);
                    assert!(matches!(&*inner.first, PaneNode::Pane(l) if l.id == "p1"));
                    assert!(matches!(&*inner.second, PaneNode::Pane(l) if l.id == second_id));
                }
                _ => panic!("first child must be the nested split"),
            },
            _ => panic!("root must be a split"),
        }
        check_invariants(&root).unwrap();
    }

    #[test]
    fn split_unknown_pane_fails() {
        let mut root = single_root();
        let err = split_pane(&mut root, "nope", Direction::Row, new_pane_leaf("C:/")).unwrap_err();
        assert_eq!(err, LayoutError::PaneNotFound("nope".into()));
    }

    #[test]
    fn close_promotes_sibling_to_root() {
        let mut root = single_root();
        let new_id = split_pane(&mut root, "p1", Direction::Row, new_pane_leaf("C:/")).unwrap();
        let removed = close_pane(&mut root, "p1").unwrap();
        assert_eq!(removed.id, "p1");
        assert!(matches!(&root, PaneNode::Pane(l) if l.id == new_id));
        check_invariants(&root).unwrap();
    }

    #[test]
    fn close_promotes_sibling_subtree() {
        // root(row) -> [ p1, split(column)->[p2, p3] ]; closing p1 must promote
        // the whole column split to root.
        let mut root = single_root();
        split_pane(&mut root, "p1", Direction::Row, new_pane_leaf("C:/")).unwrap();
        let p2 = match &root {
            PaneNode::Split(s) => match &*s.second {
                PaneNode::Pane(l) => l.id.clone(),
                _ => panic!(),
            },
            _ => panic!(),
        };
        split_pane(&mut root, &p2, Direction::Column, new_pane_leaf("C:/")).unwrap();
        close_pane(&mut root, "p1").unwrap();
        match &root {
            PaneNode::Split(s) => {
                assert_eq!(s.direction, Direction::Column);
                assert!(matches!(&*s.first, PaneNode::Pane(l) if l.id == p2));
            }
            _ => panic!("promoted subtree must be root"),
        }
        check_invariants(&root).unwrap();
        assert_eq!(collect_panes(&root).len(), 2);
    }

    #[test]
    fn close_deep_pane_keeps_invariants() {
        let mut root = single_root();
        split_pane(&mut root, "p1", Direction::Row, new_pane_leaf("C:/")).unwrap();
        let ids: Vec<String> = collect_panes(&root).iter().map(|l| l.id.clone()).collect();
        split_pane(&mut root, &ids[1], Direction::Column, new_pane_leaf("C:/")).unwrap();
        let all: Vec<String> = collect_panes(&root).iter().map(|l| l.id.clone()).collect();
        assert_eq!(all.len(), 3);
        close_pane(&mut root, &all[2]).unwrap();
        check_invariants(&root).unwrap();
        assert_eq!(collect_panes(&root).len(), 2);
        // no unary splits possible: every remaining split has 2 children by type,
        // and the promoted node replaced its parent.
    }

    #[test]
    fn close_last_pane_is_refused() {
        let mut root = single_root();
        assert_eq!(close_pane(&mut root, "p1").unwrap_err(), LayoutError::LastPane);
        assert!(matches!(&root, PaneNode::Pane(l) if l.id == "p1"));
    }

    #[test]
    fn close_unknown_pane_fails() {
        let mut root = single_root();
        split_pane(&mut root, "p1", Direction::Row, new_pane_leaf("C:/")).unwrap();
        assert_eq!(
            close_pane(&mut root, "ghost").unwrap_err(),
            LayoutError::PaneNotFound("ghost".into())
        );
    }

    #[test]
    fn resize_clamps_ratio() {
        let mut root = single_root();
        split_pane(&mut root, "p1", Direction::Row, new_pane_leaf("C:/")).unwrap();
        let split_id = match &root {
            PaneNode::Split(s) => s.id.clone(),
            _ => panic!(),
        };
        assert_eq!(resize_split(&mut root, &split_id, 0.05).unwrap(), RATIO_MIN);
        assert_eq!(resize_split(&mut root, &split_id, 0.95).unwrap(), RATIO_MAX);
        assert_eq!(resize_split(&mut root, &split_id, 0.3).unwrap(), 0.3);
        assert_eq!(resize_split(&mut root, &split_id, f32::NAN).unwrap(), DEFAULT_RATIO);
        check_invariants(&root).unwrap();
    }

    #[test]
    fn resize_unknown_split_fails() {
        let mut root = single_root();
        assert_eq!(
            resize_split(&mut root, "nope", 0.4).unwrap_err(),
            LayoutError::SplitNotFound("nope".into())
        );
    }

    #[test]
    fn scripted_op_sequence_keeps_invariants() {
        let mut root = single_root();
        for i in 0..20 {
            let panes: Vec<String> = collect_panes(&root).iter().map(|l| l.id.clone()).collect();
            let target = &panes[i % panes.len()];
            let dir = if i % 2 == 0 { Direction::Row } else { Direction::Column };
            split_pane(&mut root, target, dir, new_pane_leaf("C:/")).unwrap();
            check_invariants(&root).unwrap();
        }
        assert_eq!(collect_panes(&root).len(), 21);
        for i in 0..15 {
            let panes: Vec<String> = collect_panes(&root).iter().map(|l| l.id.clone()).collect();
            let target = panes[(i * 7) % panes.len()].clone();
            close_pane(&mut root, &target).unwrap();
            check_invariants(&root).unwrap();
        }
        assert_eq!(collect_panes(&root).len(), 6);
    }
}
