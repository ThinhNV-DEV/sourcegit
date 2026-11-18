//! Commit graph layout. Same lane algorithm as SourceGit's CommitGraph (see capture/README.md),
//! rewritten in Rust. X is in pixels, Y is in rows (row i is centered at i + 0.5).

use crate::consts::{GRAPH_HALF_WIDTH, GRAPH_PEN_COUNT, GRAPH_START_OFFSET, GRAPH_UNIT_WIDTH};
use crate::git::Commit;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug)]
pub struct Path {
    pub points: Vec<Pt>,
    pub color: usize,
    pub min_y: f32,
    pub max_y: f32,
}

#[derive(Clone, Debug)]
pub struct Link {
    pub start: Pt,
    pub control: Pt,
    pub end: Pt,
    pub color: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DotKind {
    Default,
    Head,
    Merge,
}

#[derive(Clone, Debug)]
pub struct Dot {
    pub center: Pt,
    pub color: usize,
    pub kind: DotKind,
}

#[derive(Default, Clone, Debug)]
pub struct Graph {
    pub paths: Vec<Path>,
    pub links: Vec<Link>,
    pub dots: Vec<Dot>,
}

struct ColorPicker {
    queue: VecDeque<usize>,
}

impl ColorPicker {
    fn next(&mut self) -> usize {
        if self.queue.is_empty() {
            self.queue.extend(0..GRAPH_PEN_COUNT);
        }
        self.queue.pop_front().unwrap_or(0)
    }
    fn recycle(&mut self, c: usize) {
        if !self.queue.contains(&c) {
            self.queue.push_back(c);
        }
    }
}

struct Lane {
    path: usize,
    next: String,
    last_x: f32,
    last_y: f32,
}

impl Lane {
    fn add(paths: &mut [Path], idx: usize, x: f32, y: f32) {
        let p = &mut paths[idx];
        if let Some(l) = p.points.last() {
            if (l.x - x).abs() < 1e-4 && (l.y - y).abs() < 1e-4 {
                return;
            }
        }
        p.points.push(Pt { x, y });
    }

    fn pass(&mut self, paths: &mut [Path], x: f32, mut y: f32, half: f32) {
        if x > self.last_x {
            Self::add(paths, self.path, self.last_x, self.last_y);
            Self::add(paths, self.path, x, y - half);
        } else if x < self.last_x {
            Self::add(paths, self.path, self.last_x, y - half);
            y += half;
            Self::add(paths, self.path, x, y);
        }
        self.last_x = x;
        self.last_y = y;
    }

    fn goto(&mut self, paths: &mut [Path], x: f32, y: f32, half: f32) {
        if x > self.last_x {
            Self::add(paths, self.path, self.last_x, self.last_y);
            Self::add(paths, self.path, x, y - half);
        } else if x < self.last_x {
            let mut min_y = y - half;
            if min_y > self.last_y {
                min_y -= half;
            }
            Self::add(paths, self.path, self.last_x, min_y);
            Self::add(paths, self.path, x, y);
        }
        self.last_x = x;
        self.last_y = y;
    }

    fn end(&mut self, paths: &mut [Path], x: f32, y: f32, half: f32) {
        if x > self.last_x {
            Self::add(paths, self.path, self.last_x, self.last_y);
            Self::add(paths, self.path, x, y - half);
        } else if x < self.last_x {
            Self::add(paths, self.path, self.last_x, y - half);
        }
        Self::add(paths, self.path, x, y);
        self.last_x = x;
        self.last_y = y;
    }
}

fn new_path(paths: &mut Vec<Path>, color: usize, pts: &[Pt]) -> usize {
    paths.push(Path { points: pts.to_vec(), color, min_y: 0.0, max_y: 0.0 });
    paths.len() - 1
}

/// Lays out `commits` (in display order). Also fills `commit.color` and `commit.left_margin`.
pub fn generate(commits: &mut [Commit], first_parent_only: bool) -> Graph {
    let unit_w = GRAPH_UNIT_WIDTH;
    let half_w = GRAPH_HALF_WIDTH;
    let unit_h = 1.0f32;
    let half_h = 0.5f32;

    let mut g = Graph::default();
    let mut unsolved: Vec<Lane> = Vec::new();
    let mut picker = ColorPicker { queue: VecDeque::new() };
    let mut offset_y = -half_h;

    for commit in commits.iter_mut() {
        let mut major: Option<usize> = None; // index into unsolved
        offset_y += unit_h;

        let mut offset_x = GRAPH_START_OFFSET - half_w;
        let max_offset_old = unsolved.last().map(|l| l.last_x).unwrap_or(offset_x + unit_w);
        let mut ended: Vec<usize> = Vec::new();

        for i in 0..unsolved.len() {
            if unsolved[i].next == commit.sha {
                if major.is_none() {
                    offset_x += unit_w;
                    major = Some(i);
                    if let Some(p0) = commit.parents.first() {
                        unsolved[i].next = p0.clone();
                        unsolved[i].goto(&mut g.paths, offset_x, offset_y, half_h);
                    } else {
                        unsolved[i].end(&mut g.paths, offset_x, offset_y, half_h);
                        ended.push(i);
                    }
                } else {
                    let mx = unsolved[major.unwrap()].last_x;
                    unsolved[i].end(&mut g.paths, mx, offset_y, half_h);
                    ended.push(i);
                }
            } else {
                offset_x += unit_w;
                unsolved[i].pass(&mut g.paths, offset_x, offset_y, half_h);
            }
        }

        // Position/color of the major lane before removals shift indices.
        let major_info = major.map(|i| (unsolved[i].last_x, g.paths[unsolved[i].path].color, ended.contains(&i)));

        for &i in ended.iter().rev() {
            picker.recycle(g.paths[unsolved[i].path].color);
            unsolved.remove(i);
        }

        let (position, dot_color) = match major_info {
            Some((x, c, _)) => (Pt { x, y: offset_y }, c),
            None => {
                offset_x += unit_w;
                let pos = Pt { x: offset_x, y: offset_y };
                let mut color = 0;
                if let Some(p0) = commit.parents.first() {
                    color = picker.next();
                    let path = new_path(&mut g.paths, color, &[pos]);
                    unsolved.push(Lane { path, next: p0.clone(), last_x: pos.x, last_y: pos.y });
                }
                (pos, color)
            }
        };

        let kind = if commit.is_current_head() {
            DotKind::Head
        } else if commit.parents.len() > 1 {
            DotKind::Merge
        } else {
            DotKind::Default
        };
        g.dots.push(Dot { center: position, color: dot_color, kind });

        if !first_parent_only {
            for parent in commit.parents.iter().skip(1) {
                if let Some(l) = unsolved.iter().find(|l| &l.next == parent) {
                    g.links.push(Link {
                        start: position,
                        end: Pt { x: l.last_x, y: offset_y + half_h },
                        control: Pt { x: l.last_x, y: position.y },
                        color: g.paths[l.path].color,
                    });
                } else {
                    offset_x += unit_w;
                    let color = picker.next();
                    let to = Pt { x: offset_x, y: position.y + half_h };
                    let path = new_path(&mut g.paths, color, &[position, to]);
                    unsolved.push(Lane { path, next: parent.clone(), last_x: to.x, last_y: to.y });
                }
            }
        }

        commit.color = dot_color;
        commit.left_margin = offset_x.max(max_offset_old) + half_w + 2.0;
    }

    // Lanes that leave the loaded range run to the bottom edge.
    let end_y = (commits.len() as f32 - 0.5) * unit_h;
    for (i, l) in unsolved.iter_mut().enumerate() {
        let p = &g.paths[l.path];
        if p.points.len() == 1 && (p.points[0].y - end_y).abs() < 1e-4 {
            continue;
        }
        let x = (i as f32 + 0.5) * unit_w + GRAPH_START_OFFSET;
        l.end(&mut g.paths, x, end_y + half_h, half_h);
    }

    for p in g.paths.iter_mut() {
        p.min_y = p.points.iter().map(|p| p.y).fold(f32::MAX, f32::min);
        p.max_y = p.points.iter().map(|p| p.y).fold(f32::MIN, f32::max);
    }
    g.paths.retain(|p| p.points.len() >= 2);
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(sha: &str, parents: &[&str]) -> Commit {
        Commit { sha: sha.into(), parents: parents.iter().map(|s| s.to_string()).collect(), ..Default::default() }
    }

    #[test]
    fn linear_history_single_lane() {
        let mut v = vec![c("c", &["b"]), c("b", &["a"]), c("a", &[])];
        let g = generate(&mut v, false);
        assert_eq!(g.dots.len(), 3);
        assert!(g.dots.iter().all(|d| (d.center.x - g.dots[0].center.x).abs() < 1e-4));
        assert!(g.links.is_empty());
    }

    #[test]
    fn merge_creates_second_lane() {
        // m merges b2 into b1
        let mut v = vec![c("m", &["b1", "b2"]), c("b2", &["a"]), c("b1", &["a"]), c("a", &[])];
        let g = generate(&mut v, false);
        assert_eq!(g.dots[0].kind, DotKind::Merge);
        assert!(g.dots[1].center.x > g.dots[0].center.x, "second parent drawn in a lane to the right");
        assert!(v[0].left_margin > 0.0);
    }
}
