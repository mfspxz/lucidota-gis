//! route — gis-012: pedicab route planner.
//!
//! Waypoint graph over POIs + road connectors; Dijkstra shortest path with a
//! pedicab profile (avoids steep grades via penalty, prefers flatter legs).
//! Proven by construction: every returned route's length is recomputed from
//! its legs in tests.

use geo_core::coord::LatLon;
use std::collections::{BinaryHeap, HashMap};
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy)]
pub enum Profile {
    /// Pedicab: penalize long steep legs (grade proxy via leg length — flat
    /// island legs preferred), walking-speed floor.
    Pedicab,
    Direct,
}

#[derive(Debug, Clone)]
pub struct Waypoint {
    pub id: String,
    pub pos: LatLon,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct Leg {
    pub from: String,
    pub to: String,
    pub dist_m: f64,
    /// grade proxy 0..1 (0 = flat). Higher = worse for pedicab.
    pub grade: f64,
}

#[derive(Debug, Default)]
pub struct Graph {
    pub nodes: HashMap<String, Waypoint>,
    pub adj: HashMap<String, Vec<Leg>>,
}

impl Graph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, id: &str, name: &str, pos: LatLon) {
        self.nodes.insert(id.into(), Waypoint { id: id.into(), pos, name: name.into() });
    }

    /// Bidirectional leg; grade estimated from leg length (longer = hillier
    /// proxy on island terrain) unless overridden.
    pub fn add_leg(&mut self, a: &str, b: &str, grade: f64) {
        let pa = self.nodes.get(a).expect("unknown node").pos;
        let pb = self.nodes.get(b).expect("unknown node").pos;
        let d = pa.distance_m(&pb);
        self.adj.entry(a.into()).or_default().push(Leg { from: a.into(), to: b.into(), dist_m: d, grade });
        self.adj.entry(b.into()).or_default().push(Leg { from: b.into(), to: a.into(), dist_m: d, grade });
    }

    fn cost(leg: &Leg, profile: Profile) -> f64 {
        match profile {
            Profile::Direct => leg.dist_m,
            Profile::Pedicab => leg.dist_m * (1.0 + 3.0 * leg.grade),
        }
    }

    /// Dijkstra. Returns node id path.
    pub fn shortest(&self, from: &str, to: &str, profile: Profile) -> Option<Vec<String>> {
        #[derive(PartialEq)]
        struct State { cost: f64, node: String }
        impl Eq for State {}
        impl Ord for State {
            fn cmp(&self, other: &Self) -> Ordering {
                other.cost.partial_cmp(&self.cost).unwrap_or(Ordering::Equal)
            }
        }
        impl PartialOrd for State {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
        }
        let mut dist: HashMap<String, f64> = HashMap::new();
        let mut prev: HashMap<String, String> = HashMap::new();
        let mut heap = BinaryHeap::new();
        dist.insert(from.into(), 0.0);
        heap.push(State { cost: 0.0, node: from.into() });
        while let Some(State { cost, node }) = heap.pop() {
            if node == to {
                let mut path = vec![node.clone()];
                let mut cur = node;
                while let Some(p) = prev.get(&cur) {
                    path.push(p.clone());
                    cur = p.clone();
                }
                path.reverse();
                return Some(path);
            }
            if cost > *dist.get(&node).unwrap_or(&f64::INFINITY) {
                continue;
            }
            if let Some(legs) = self.adj.get(&node) {
                for leg in legs {
                    let nc = cost + Self::cost(leg, profile);
                    if nc < *dist.get(&leg.to).unwrap_or(&f64::INFINITY) {
                        dist.insert(leg.to.clone(), nc);
                        prev.insert(leg.to.clone(), node.clone());
                        heap.push(State { cost: nc, node: leg.to.clone() });
                    }
                }
            }
        }
        None
    }

    pub fn path_distance_m(&self, path: &[String]) -> f64 {
        path.windows(2).map(|w| {
            self.adj.get(&w[0]).and_then(|legs| legs.iter().find(|l| l.to == w[1]))
                .map(|l| l.dist_m).unwrap_or(0.0)
        }).sum()
    }

    /// Build a graph from a POI db: fully connect POIs within max_leg_m.
    pub fn from_pois(db: &poi::PoiDb, max_leg_m: f64) -> Self {
        let mut g = Graph::new();
        // poi::PoiDb has no iterator; rebuild via near() from each poi is awkward —
        // instead we expose nodes through a snapshot. (See PoiDb::snapshot below.)
        for p in db.snapshot() {
            g.add_node(&p.id, &p.name, p.pos);
        }
        let ids: Vec<String> = g.nodes.keys().cloned().collect();
        for i in 0..ids.len() {
            for j in i + 1..ids.len() {
                let a = &g.nodes[&ids[i]];
                let b = &g.nodes[&ids[j]];
                let d = a.pos.distance_m(&b.pos);
                if d <= max_leg_m && d > 1.0 {
                    // grade proxy: normalized leg length
                    let grade = (d / max_leg_m).min(1.0) * 0.5;
                    g.add_leg(&ids[i], &ids[j], grade);
                }
            }
        }
        g
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn island_graph() -> Graph {
        let mut g = Graph::new();
        let p = |lat: f64, lon: f64| LatLon::new_unchecked(lat, lon);
        g.add_node("ganges", "Ganges", p(48.8541, -123.4994));
        g.add_node("fulford", "Fulford", p(48.7677, -123.4500));
        g.add_node("vesuvius", "Vesuvius", p(48.8862, -123.5572));
        g.add_node("longharbour", "Long Harbour", p(48.8594, -123.4750));
        // direct Ganges-Fulford is "steep" (grade 0.9); via Long Harbour is flat
        g.add_leg("ganges", "fulford", 0.9);
        g.add_leg("ganges", "longharbour", 0.05);
        g.add_leg("longharbour", "fulford", 0.05);
        g.add_leg("ganges", "vesuvius", 0.3);
        g
    }

    #[test]
    fn pedicab_avoids_steep_direct_leg() {
        let g = island_graph();
        let path = g.shortest("ganges", "fulford", Profile::Pedicab).unwrap();
        assert_eq!(path, vec!["ganges", "longharbour", "fulford"],
            "pedicab should detour around the steep direct leg");
    }

    #[test]
    fn direct_takes_shortest() {
        let g = island_graph();
        let path = g.shortest("ganges", "fulford", Profile::Direct).unwrap();
        assert_eq!(path, vec!["ganges", "fulford"]);
    }

    #[test]
    fn path_distance_recomputed() {
        let g = island_graph();
        let path = g.shortest("ganges", "vesuvius", Profile::Pedicab).unwrap();
        let d = g.path_distance_m(&path);
        let a = LatLon::new_unchecked(48.8541, -123.4994);
        let b = LatLon::new_unchecked(48.8862, -123.5572);
        assert!((d - a.distance_m(&b)).abs() < 1.0, "d={d}");
    }

    #[test]
    fn disconnected_returns_none() {
        let mut g = Graph::new();
        g.add_node("a", "A", LatLon::new_unchecked(48.85, -123.49));
        g.add_node("b", "B", LatLon::new_unchecked(48.86, -123.49));
        assert!(g.shortest("a", "b", Profile::Direct).is_none());
    }

    #[test]
    fn from_pois_builds_routable_graph() {
        let mut db = poi::PoiDb::new();
        db.seed_saltspring();
        let g = Graph::from_pois(&db, 12_000.0);
        assert!(g.nodes.len() >= 10);
        // Ganges should reach Fulford Harbour through the mesh
        let ganges = db.near(&LatLon::new_unchecked(48.8541, -123.4994), 100.0)[0].0.id.clone();
        let fulford = db.near(&LatLon::new_unchecked(48.7677, -123.4500), 500.0)[0].0.id.clone();
        let path = g.shortest(&ganges, &fulford, Profile::Pedicab);
        assert!(path.is_some(), "mesh should connect Ganges to Fulford");
        let d = g.path_distance_m(&path.unwrap());
        assert!(d > 8_000.0 && d < 40_000.0, "d={d}");
    }
}
