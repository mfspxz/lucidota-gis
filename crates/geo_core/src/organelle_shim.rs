//! Organelle-compatibility shim for GIS ops.
//!
//! gis-008 ("GIS as organelles") depends on org-002, which lives in another
//! domain builder and is not available in this session. This shim defines the
//! exact trait surface the geo ops will expose to the organelle host, so the
//! ops are *already* pure, stateless, and named — mechanical registration
//! later, zero drift. The shim MUST NOT grow a second authority model:
//! when org-002 lands, this module is deleted and ops register directly.

use std::collections::HashMap;

/// A pure geo operation: named, stateless, string-in/string-out contract.
pub trait GeoOp: Send + Sync {
    fn name(&self) -> &'static str;
    /// args are (lat1, lon1, lat2, lon2) in degrees; result in meters.
    fn run(&self, args: &[f64]) -> Result<f64, String>;
}

pub struct HaversineOp;
impl GeoOp for HaversineOp {
    fn name(&self) -> &'static str {
        "geo.haversine_m"
    }
    fn run(&self, args: &[f64]) -> Result<f64, String> {
        if args.len() != 4 {
            return Err("need 4 args".into());
        }
        let a = crate::coord::LatLon::new(args[0], args[1]).ok_or("bad point A")?;
        let b = crate::coord::LatLon::new(args[2], args[3]).ok_or("bad point B")?;
        Ok(a.distance_m(&b))
    }
}

pub struct TileXYOp;
impl GeoOp for TileXYOp {
    fn name(&self) -> &'static str {
        "geo.tile_xy"
    }
    fn run(&self, args: &[f64]) -> Result<f64, String> {
        if args.len() != 3 {
            return Err("need lat,lon,z".into());
        }
        let p = crate::coord::LatLon::new(args[0], args[1]).ok_or("bad point")?;
        let (x, y) = crate::coord::WebMercator::tile_xy(&p, args[2] as u8);
        Ok(x as f64 * 1e6 + y as f64) // packed x*1e6+y
    }
}

pub struct BearingOp;
impl GeoOp for BearingOp {
    fn name(&self) -> &'static str {
        "geo.bearing_deg"
    }
    fn run(&self, args: &[f64]) -> Result<f64, String> {
        if args.len() != 4 {
            return Err("need 4 args".into());
        }
        let a = crate::coord::LatLon::new(args[0], args[1]).ok_or("bad point A")?;
        let b = crate::coord::LatLon::new(args[2], args[3]).ok_or("bad point B")?;
        Ok(a.bearing_deg(&b))
    }
}

/// Registry of available ops (stand-in for the organelle host registry).
pub struct OpRegistry {
    ops: HashMap<&'static str, Box<dyn GeoOp>>,
}

impl OpRegistry {
    pub fn builtin() -> Self {
        let mut ops: HashMap<&'static str, Box<dyn GeoOp>> = HashMap::new();
        for op in [Box::new(HaversineOp) as Box<dyn GeoOp>, Box::new(TileXYOp), Box::new(BearingOp)] {
            ops.insert(op.name(), op);
        }
        Self { ops }
    }

    pub fn call(&self, name: &str, args: &[f64]) -> Result<f64, String> {
        self.ops.get(name).ok_or_else(|| format!("unknown op {name}"))?.run(args)
    }

    pub fn names(&self) -> Vec<&'static str> {
        let mut n: Vec<_> = self.ops.keys().copied().collect();
        n.sort_unstable();
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_dispatches() {
        let r = OpRegistry::builtin();
        let d = r.call("geo.haversine_m", &[48.85, -123.49, 48.86, -123.49]).unwrap();
        assert!(d > 1_000.0 && d < 1_300.0);
        assert!(r.call("nope", &[]).is_err());
    }

    #[test]
    fn ops_reject_bad_args() {
        let r = OpRegistry::builtin();
        assert!(r.call("geo.haversine_m", &[91.0, 0.0, 0.0, 0.0]).is_err());
        assert!(r.call("geo.tile_xy", &[48.85]).is_err());
    }
}
