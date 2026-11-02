//! Analytic carrier definitions and source-preserving native populations.
//! Carrier domains are distinct from trimmed face/edge domains.
use crate::{DiagnosticCode, DocumentInfo, metadata::StreamSpan};
use serde::{Deserialize, Serialize};
mod intersection;
mod jet;
mod nurbs;
mod rolling;
mod ruled;
mod search;
pub use jet::UvJet;
pub use nurbs::{NurbsCurve, NurbsSurface};
pub use rolling::RollingBallJet;
pub use ruled::RuledJet;

pub type Vec3 = [f64; 3];
/// Evaluate a normalized interval coordinate without rounding its declared
/// endpoints out of the interval. Interior coordinates keep their original
/// formula. This is not a clamp for arbitrary source parameters.
pub(crate) fn interval_parameter(range: [f64; 2], q: f64) -> f64 {
    if q == 0. {
        range[0]
    } else if q == 1. {
        range[1]
    } else {
        range[0] + q * (range[1] - range[0])
    }
}
pub(crate) fn add(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|i| a[i] + b[i])
}
pub(crate) fn sub(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|i| a[i] - b[i])
}
pub(crate) fn scale(a: Vec3, s: f64) -> Vec3 {
    a.map(|v| v * s)
}
pub(crate) fn dot(a: Vec3, b: Vec3) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}
pub(crate) fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub(crate) fn finite(a: Vec3) -> bool {
    a.into_iter().all(f64::is_finite)
}
pub(crate) fn unit(a: Vec3) -> Option<Vec3> {
    if !finite(a) {
        return None;
    }
    let maximum = a.into_iter().map(f64::abs).fold(0., f64::max);
    if maximum == 0. {
        return None;
    }
    let b = a.map(|v| v / maximum);
    let n = dot(b, b).sqrt();
    Some(scale(b, 1. / n))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationError {
    InvalidGeometry,
    InvalidParameter,
    OutsideDomain,
    UnresolvedGeometry,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame3 {
    pub origin: Vec3,
    pub x_axis: Vec3,
    pub y_axis: Vec3,
    pub z_axis: Vec3,
}
impl Frame3 {
    #[must_use]
    pub fn identity() -> Self {
        Self {
            origin: [0.; 3],
            x_axis: [1., 0., 0.],
            y_axis: [0., 1., 0.],
            z_axis: [0., 0., 1.],
        }
    }
    /// Derives a reproducible chart; this does not assert native UV directions.
    pub fn from_axis(origin: Vec3, axis: Vec3) -> Option<Self> {
        if !finite(origin) {
            return None;
        }
        let z_axis = unit(axis)?;
        let index = (0..3).min_by(|a, b| z_axis[*a].abs().total_cmp(&z_axis[*b].abs()))?;
        let mut seed = [0.; 3];
        seed[index] = 1.;
        let x_axis = unit(sub(seed, scale(z_axis, dot(seed, z_axis))))?;
        let y_axis = cross(z_axis, x_axis);
        Some(Self {
            origin,
            x_axis,
            y_axis,
            z_axis,
        })
    }
    #[must_use]
    pub fn is_valid(&self) -> bool {
        finite(self.origin)
            && [self.x_axis, self.y_axis, self.z_axis]
                .into_iter()
                .all(|a| finite(a) && (dot(a, a) - 1.).abs() < 1e-10)
            && (dot(self.x_axis, self.y_axis)).abs() < 1e-10
            && dot(
                sub(cross(self.x_axis, self.y_axis), self.z_axis),
                sub(cross(self.x_axis, self.y_axis), self.z_axis),
            ) < 1e-20
    }
    fn point(&self, x: f64, y: f64, z: f64) -> Vec3 {
        add(
            self.origin,
            add(
                scale(self.x_axis, x),
                add(scale(self.y_axis, y), scale(self.z_axis, z)),
            ),
        )
    }
    pub fn transformed(&self, placement: &RigidTransform) -> Result<Self, EvaluationError> {
        if !self.is_valid() || !placement.is_valid() {
            return Err(EvaluationError::InvalidGeometry);
        }
        let output = Self {
            origin: placement.point(self.origin),
            x_axis: placement.vector(self.x_axis),
            y_axis: placement.vector(self.y_axis),
            z_axis: placement.vector(self.z_axis),
        };
        if output.is_valid() {
            Ok(output)
        } else {
            Err(EvaluationError::InvalidGeometry)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigidTransform {
    pub translation: Vec3,
    pub rotation: [[f64; 3]; 3],
}
impl RigidTransform {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        Frame3 {
            origin: self.translation,
            x_axis: self.rotation[0],
            y_axis: self.rotation[1],
            z_axis: self.rotation[2],
        }
        .is_valid()
    }
    /// Rotation stores the images of the three coordinate axes as columns.
    fn vector(&self, p: Vec3) -> Vec3 {
        add(
            scale(self.rotation[0], p[0]),
            add(scale(self.rotation[1], p[1]), scale(self.rotation[2], p[2])),
        )
    }
    fn point(&self, p: Vec3) -> Vec3 {
        add(self.translation, self.vector(p))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ParameterDomain {
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub period: Option<f64>,
}
impl ParameterDomain {
    const UNBOUNDED: Self = Self {
        minimum: None,
        maximum: None,
        period: None,
    };
    const ANGLE: Self = Self {
        minimum: Some(0.),
        maximum: Some(std::f64::consts::TAU),
        period: Some(std::f64::consts::TAU),
    };
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CurveGeometry {
    RollingIntersection {
        surface: Box<RollingBallJet>,
        support: Box<SurfaceGeometry>,
        u_range: [f64; 2],
        phi_hints: [f64; 2],
    },
    SurfaceJet {
        surface: Box<SurfaceGeometry>,
        jet: UvJet,
        uv_scale: [f64; 2],
        #[serde(default)]
        uv_offset: [f64; 2],
    },
    Nurbs {
        curve: NurbsCurve,
    },
    /// A collapsed edge locus, with its nontrivial UV use carried by topology.
    Point {
        position: Vec3,
    },
    Line {
        origin: Vec3,
        direction: Vec3,
    },
    Circle {
        frame: Frame3,
        radius: f64,
    },
    Ellipse {
        frame: Frame3,
        major_radius: f64,
        minor_radius: f64,
    },
    /// One exact quadratic-root branch of two nonparallel circular cylinders.
    CylinderIntersection {
        primary: Frame3,
        primary_radius: f64,
        secondary: Frame3,
        secondary_radius: f64,
        branch: i8,
    },
    Unknown {
        reason: DiagnosticCode,
    },
}
impl CurveGeometry {
    #[must_use]
    pub fn domain(&self) -> Option<ParameterDomain> {
        match self {
            Self::RollingIntersection { .. } => Some(ParameterDomain {
                minimum: Some(0.),
                maximum: Some(1.),
                period: None,
            }),
            Self::SurfaceJet { jet, .. } => Some(ParameterDomain {
                minimum: Some(jet.parameter_range[0]),
                maximum: Some(jet.parameter_range[1]),
                period: None,
            }),
            Self::Nurbs { curve } => curve.range().map(|[minimum, maximum]| ParameterDomain {
                minimum: Some(minimum),
                maximum: Some(maximum),
                period: None,
            }),
            Self::Point { .. } => Some(ParameterDomain {
                minimum: Some(0.),
                maximum: Some(1.),
                period: None,
            }),
            Self::Line { .. } => Some(ParameterDomain::UNBOUNDED),
            Self::Circle { .. } => Some(ParameterDomain::ANGLE),
            Self::Ellipse { .. } | Self::CylinderIntersection { .. } => {
                Some(ParameterDomain::ANGLE)
            }
            Self::Unknown { .. } => None,
        }
    }
    pub fn evaluate(&self, t: f64) -> Result<Vec3, EvaluationError> {
        if !t.is_finite() {
            return Err(EvaluationError::InvalidParameter);
        }
        let p = match self {
            Self::RollingIntersection {
                surface,
                support,
                u_range,
                phi_hints,
            } => intersection::rolling(surface, support, *u_range, *phi_hints, t)?,
            Self::SurfaceJet {
                surface,
                jet,
                uv_scale,
                uv_offset,
            } => {
                if uv_scale.iter().any(|v| !v.is_finite() || *v == 0.)
                    || uv_offset.iter().any(|v| !v.is_finite())
                {
                    return Err(EvaluationError::InvalidGeometry);
                }
                let uv = jet.evaluate(t)?;
                surface.evaluate([
                    uv[0] * uv_scale[0] + uv_offset[0],
                    uv[1] * uv_scale[1] + uv_offset[1],
                ])?
            }
            Self::Nurbs { curve } => curve.evaluate(t)?,
            Self::Point { position } if finite(*position) => {
                if !(0. ..=1.).contains(&t) {
                    return Err(EvaluationError::OutsideDomain);
                }
                *position
            }
            Self::Line { origin, direction }
                if finite(*origin)
                    && finite(*direction)
                    && (dot(*direction, *direction) - 1.).abs() < 1e-10 =>
            {
                add(*origin, scale(*direction, t))
            }
            Self::Circle { frame, radius }
                if frame.is_valid() && radius.is_finite() && *radius > 0. =>
            {
                frame.point(radius * t.cos(), radius * t.sin(), 0.)
            }
            Self::Ellipse {
                frame,
                major_radius,
                minor_radius,
            } if frame.is_valid()
                && major_radius.is_finite()
                && minor_radius.is_finite()
                && *major_radius > 0.
                && *minor_radius > 0. =>
            {
                frame.point(major_radius * t.cos(), minor_radius * t.sin(), 0.)
            }
            Self::CylinderIntersection {
                primary,
                primary_radius,
                secondary,
                secondary_radius,
                branch,
            } => {
                if !primary.is_valid()
                    || !secondary.is_valid()
                    || !primary_radius.is_finite()
                    || !secondary_radius.is_finite()
                    || *primary_radius <= 0.
                    || *secondary_radius <= 0.
                    || !matches!(branch, 1 | -1)
                {
                    return Err(EvaluationError::InvalidGeometry);
                }
                let radial = primary.point(primary_radius * t.cos(), primary_radius * t.sin(), 0.);
                let d = sub(radial, secondary.origin);
                let perpendicular =
                    |p: Vec3| sub(p, scale(secondary.z_axis, dot(p, secondary.z_axis)));
                let axis = perpendicular(primary.z_axis);
                let delta = perpendicular(d);
                let a = dot(axis, axis);
                let b = 2. * dot(axis, delta);
                let c = dot(delta, delta) - secondary_radius * secondary_radius;
                if ![a, b, c].into_iter().all(f64::is_finite) {
                    return Err(EvaluationError::InvalidGeometry);
                }
                if a < 1e-12 {
                    return Err(EvaluationError::InvalidGeometry);
                }
                let disc = b * b - 4. * a * c;
                if !disc.is_finite() {
                    return Err(EvaluationError::InvalidGeometry);
                }
                let rounding = 128. * f64::EPSILON * (b * b + (4. * a * c).abs()).max(1.);
                if disc < -rounding {
                    return Err(EvaluationError::OutsideDomain);
                }
                let z = (-b + f64::from(*branch) * disc.max(0.).sqrt()) / (2. * a);
                add(radial, scale(primary.z_axis, z))
            }
            Self::Unknown { .. } => return Err(EvaluationError::UnresolvedGeometry),
            _ => return Err(EvaluationError::InvalidGeometry),
        };
        if finite(p) {
            Ok(p)
        } else {
            Err(EvaluationError::InvalidGeometry)
        }
    }
    pub fn transformed(&self, t: &RigidTransform) -> Result<Self, EvaluationError> {
        if !t.is_valid() {
            return Err(EvaluationError::InvalidGeometry);
        }
        if !matches!(
            self,
            Self::CylinderIntersection { .. }
                | Self::Nurbs { .. }
                | Self::SurfaceJet { .. }
                | Self::RollingIntersection { .. }
        ) {
            self.evaluate(0.)?;
        }
        let output = match self {
            Self::RollingIntersection {
                surface,
                support,
                u_range,
                phi_hints,
            } => Self::RollingIntersection {
                surface: Box::new(surface.transformed(t)?),
                support: Box::new(support.transformed(t)?),
                u_range: *u_range,
                phi_hints: *phi_hints,
            },
            Self::SurfaceJet {
                surface,
                jet,
                uv_scale,
                uv_offset,
            } => Self::SurfaceJet {
                surface: Box::new(surface.transformed(t)?),
                jet: jet.clone(),
                uv_scale: *uv_scale,
                uv_offset: *uv_offset,
            },
            Self::Nurbs { curve } => Self::Nurbs {
                curve: curve.transformed(t)?,
            },
            Self::Point { position } => Self::Point {
                position: t.point(*position),
            },
            Self::Line { origin, direction } => Self::Line {
                origin: t.point(*origin),
                direction: t.vector(*direction),
            },
            Self::Circle { frame, radius } => Self::Circle {
                frame: frame.transformed(t)?,
                radius: *radius,
            },
            Self::Ellipse {
                frame,
                major_radius,
                minor_radius,
            } => Self::Ellipse {
                frame: frame.transformed(t)?,
                major_radius: *major_radius,
                minor_radius: *minor_radius,
            },
            Self::CylinderIntersection {
                primary,
                primary_radius,
                secondary,
                secondary_radius,
                branch,
            } => {
                if !primary_radius.is_finite()
                    || !secondary_radius.is_finite()
                    || *primary_radius <= 0.
                    || *secondary_radius <= 0.
                    || !matches!(branch, 1 | -1)
                    || dot(
                        cross(primary.z_axis, secondary.z_axis),
                        cross(primary.z_axis, secondary.z_axis),
                    ) < 1e-12
                {
                    return Err(EvaluationError::InvalidGeometry);
                }
                Self::CylinderIntersection {
                    primary: primary.transformed(t)?,
                    primary_radius: *primary_radius,
                    secondary: secondary.transformed(t)?,
                    secondary_radius: *secondary_radius,
                    branch: *branch,
                }
            }
            Self::Unknown { .. } => return Err(EvaluationError::UnresolvedGeometry),
        };
        if !matches!(
            output,
            Self::CylinderIntersection { .. }
                | Self::Nurbs { .. }
                | Self::SurfaceJet { .. }
                | Self::RollingIntersection { .. }
        ) {
            output.evaluate(0.)?;
        }
        Ok(output)
    }

    /// Carrier parameter of a supplied point; callers must also check its locus.
    pub(crate) fn angular_parameter(&self, p: Vec3) -> Option<f64> {
        let (frame, x, y) = match self {
            Self::Circle { frame, radius } => (frame, *radius, *radius),
            Self::Ellipse {
                frame,
                major_radius,
                minor_radius,
            } => (frame, *major_radius, *minor_radius),
            Self::CylinderIntersection { primary, .. } => (primary, 1., 1.),
            _ => return None,
        };
        let d = sub(p, frame.origin);
        Some((dot(d, frame.y_axis) / y).atan2(dot(d, frame.x_axis) / x))
    }
    pub(crate) fn bounded_parameter(&self, p: Vec3, tolerance: f64) -> Option<f64> {
        search::parameter(self, p, tolerance)
    }
    pub(crate) fn tangent(&self, t: f64) -> Option<Vec3> {
        match self {
            Self::RollingIntersection { .. } => {
                let h = 1e-6;
                let a = (t - h).max(0.);
                let z = (t + h).min(1.);
                Some(scale(
                    sub(self.evaluate(z).ok()?, self.evaluate(a).ok()?),
                    1. / (z - a),
                ))
            }
            Self::SurfaceJet { jet, .. } => {
                let [lo, hi] = jet.parameter_range;
                let h = (hi - lo) * 1e-6;
                let a = (t - h).max(lo);
                let z = (t + h).min(hi);
                Some(scale(
                    sub(self.evaluate(z).ok()?, self.evaluate(a).ok()?),
                    1. / (z - a),
                ))
            }
            Self::Nurbs { curve } => curve.derivative(t).ok(),
            Self::Line { direction, .. } => Some(*direction),
            Self::Circle { frame, radius } => Some(add(
                scale(frame.x_axis, -radius * t.sin()),
                scale(frame.y_axis, radius * t.cos()),
            )),
            Self::Ellipse {
                frame,
                major_radius,
                minor_radius,
            } => Some(add(
                scale(frame.x_axis, -major_radius * t.sin()),
                scale(frame.y_axis, minor_radius * t.cos()),
            )),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SurfaceGeometry {
    RollingBall {
        surface: RollingBallJet,
    },
    /// Same native sphere locus, with a derived regular chart excluding -Z.
    SphereStereographic {
        frame: Frame3,
        radius: f64,
    },
    Nurbs {
        surface: NurbsSurface,
    },
    Plane {
        frame: Frame3,
    },
    Cylinder {
        frame: Frame3,
        radius: f64,
    },
    Cone {
        frame: Frame3,
        half_angle: f64,
    },
    Sphere {
        frame: Frame3,
        radius: f64,
    },
    Torus {
        frame: Frame3,
        major_radius: f64,
        minor_radius: f64,
    },
    Unknown {
        reason: DiagnosticCode,
    },
}
impl SurfaceGeometry {
    pub(crate) fn implicit_value(&self, p: Vec3) -> Option<f64> {
        intersection::implicit(self, p)
    }
    #[must_use]
    pub fn domains(&self) -> Option<[ParameterDomain; 2]> {
        use std::f64::consts::FRAC_PI_2;
        Some(match self {
            Self::RollingBall { surface } => {
                if !surface.is_valid() {
                    return None;
                }
                [
                    ParameterDomain {
                        minimum: Some(surface.parameter_range[0]),
                        maximum: Some(surface.parameter_range[1]),
                        period: None,
                    },
                    ParameterDomain {
                        minimum: Some(0.),
                        maximum: Some(1.),
                        period: None,
                    },
                ]
            }
            Self::SphereStereographic { .. } => [ParameterDomain::UNBOUNDED; 2],
            Self::Nurbs { surface } => {
                if !surface.is_valid() {
                    return None;
                }
                std::array::from_fn(|d| ParameterDomain {
                    minimum: Some(surface.knots[d][surface.degrees[d]]),
                    maximum: Some(surface.knots[d][surface.control_counts[d]]),
                    period: None,
                })
            }
            Self::Plane { .. } => [ParameterDomain::UNBOUNDED; 2],
            Self::Cylinder { .. } => [ParameterDomain::ANGLE, ParameterDomain::UNBOUNDED],
            Self::Cone { .. } => [
                ParameterDomain::ANGLE,
                ParameterDomain {
                    minimum: Some(0.),
                    ..ParameterDomain::UNBOUNDED
                },
            ],
            Self::Sphere { .. } => [
                ParameterDomain::ANGLE,
                ParameterDomain {
                    minimum: Some(-FRAC_PI_2),
                    maximum: Some(FRAC_PI_2),
                    period: None,
                },
            ],
            Self::Torus { .. } => [ParameterDomain::ANGLE; 2],
            Self::Unknown { .. } => return None,
        })
    }
    pub fn evaluate(&self, uv: [f64; 2]) -> Result<Vec3, EvaluationError> {
        let [u, v] = uv;
        if let Self::RollingBall { surface } = self {
            return surface.evaluate(uv);
        }
        if let Self::Nurbs { surface } = self {
            return surface.evaluate(uv);
        }
        if !u.is_finite() || !v.is_finite() {
            return Err(EvaluationError::InvalidParameter);
        }
        let valid_radius = |r: f64| r.is_finite() && r > 0.;
        let (frame, local) = match self {
            Self::SphereStereographic { frame, radius } if valid_radius(*radius) => {
                let denominator = 1. + u * u + v * v;
                if !denominator.is_finite() {
                    return Err(EvaluationError::InvalidParameter);
                }
                (
                    frame,
                    [
                        2. * radius * u / denominator,
                        2. * radius * v / denominator,
                        radius * (1. - u * u - v * v) / denominator,
                    ],
                )
            }
            Self::Plane { frame } => (frame, [u, v, 0.]),
            Self::Cylinder { frame, radius } if valid_radius(*radius) => {
                (frame, [radius * u.cos(), radius * u.sin(), v])
            }
            Self::Cone { frame, half_angle }
                if half_angle.is_finite()
                    && *half_angle > 0.
                    && *half_angle < std::f64::consts::FRAC_PI_2 =>
            {
                if v < 0. {
                    return Err(EvaluationError::OutsideDomain);
                }
                (
                    frame,
                    [
                        v * half_angle.sin() * u.cos(),
                        v * half_angle.sin() * u.sin(),
                        v * half_angle.cos(),
                    ],
                )
            }
            Self::Sphere { frame, radius } if valid_radius(*radius) => {
                if v.abs() > std::f64::consts::FRAC_PI_2 {
                    return Err(EvaluationError::OutsideDomain);
                }
                (
                    frame,
                    [
                        radius * v.cos() * u.cos(),
                        radius * v.cos() * u.sin(),
                        radius * v.sin(),
                    ],
                )
            }
            Self::Torus {
                frame,
                major_radius,
                minor_radius,
            } if valid_radius(*major_radius) && valid_radius(*minor_radius) => {
                let r = major_radius + minor_radius * v.cos();
                (frame, [r * u.cos(), r * u.sin(), minor_radius * v.sin()])
            }
            Self::Unknown { .. } => return Err(EvaluationError::UnresolvedGeometry),
            _ => return Err(EvaluationError::InvalidGeometry),
        };
        if !frame.is_valid() {
            return Err(EvaluationError::InvalidGeometry);
        }
        let p = frame.point(local[0], local[1], local[2]);
        if finite(p) {
            Ok(p)
        } else {
            Err(EvaluationError::InvalidGeometry)
        }
    }
    pub fn transformed(&self, t: &RigidTransform) -> Result<Self, EvaluationError> {
        if !matches!(self, Self::Nurbs { .. } | Self::RollingBall { .. }) {
            self.evaluate([0., 0.])?;
        }
        let output = match self {
            Self::SphereStereographic { frame, radius } => Self::SphereStereographic {
                frame: frame.transformed(t)?,
                radius: *radius,
            },
            Self::RollingBall { surface } => Self::RollingBall {
                surface: surface.transformed(t)?,
            },
            Self::Nurbs { surface } => Self::Nurbs {
                surface: surface.transformed(t)?,
            },
            Self::Plane { frame } => Self::Plane {
                frame: frame.transformed(t)?,
            },
            Self::Cylinder { frame, radius } => Self::Cylinder {
                frame: frame.transformed(t)?,
                radius: *radius,
            },
            Self::Cone { frame, half_angle } => Self::Cone {
                frame: frame.transformed(t)?,
                half_angle: *half_angle,
            },
            Self::Sphere { frame, radius } => Self::Sphere {
                frame: frame.transformed(t)?,
                radius: *radius,
            },
            Self::Torus {
                frame,
                major_radius,
                minor_radius,
            } => Self::Torus {
                frame: frame.transformed(t)?,
                major_radius: *major_radius,
                minor_radius: *minor_radius,
            },
            Self::Unknown { .. } => return Err(EvaluationError::UnresolvedGeometry),
        };
        output.evaluate([0., 0.])?;
        Ok(output)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeometryLayout {
    Unknown,
    StandardNested,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnitContext {
    pub length: String,
    pub angle: String,
    pub evidence: String,
    pub native_unit_declaration: Option<String>,
}
impl Default for UnitContext {
    fn default() -> Self {
        Self {
            length: "unknown".into(),
            angle: "unknown".into(),
            evidence: "unresolved".into(),
            native_unit_declaration: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeometryPopulation {
    pub id: u32,
    pub directory_id: u32,
    pub main_stream_id: u32,
    pub surface_stream_id: u32,
    pub face_count: u32,
    pub edge_count: u32,
    pub point_count: u32,
    pub handle_width: u8,
    pub face_rows: StreamSpan,
    pub edge_tables: Vec<StreamSpan>,
    pub vertex_table: StreamSpan,
    pub trim_chain: StreamSpan,
    pub surface_roster: StreamSpan,
    pub support_table: StreamSpan,
    pub body_id: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointRecord {
    pub id: String,
    pub population_id: u32,
    pub table_ordinal: u32,
    pub native_allocation_id: Option<u32>,
    pub identity_source: Option<StreamSpan>,
    pub position: Vec3,
    pub source: StreamSpan,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceRecord {
    pub id: String,
    pub population_id: u32,
    pub record_ordinal: u32,
    pub native_carrier_tag: u32,
    pub geometry: SurfaceGeometry,
    pub source: StreamSpan,
    pub evidence: Vec<StreamSpan>,
    pub raw_parameters_hex: String,
    pub domains: Option<[ParameterDomain; 2]>,
    pub parameterization: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurveRecord {
    pub id: String,
    pub population_id: u32,
    pub record_ordinal: u32,
    pub native_allocation_tag: u32,
    pub geometry: CurveGeometry,
    pub source: StreamSpan,
    pub evidence: Vec<StreamSpan>,
    pub raw_parameters_hex: String,
    pub domain: Option<ParameterDomain>,
    pub parameter_interval: Option<[f64; 2]>,
    pub endpoint_point_ids: Option<[String; 2]>,
    pub parameterization: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceUse {
    pub population_id: u32,
    pub face_ordinal: u32,
    pub definition_id: String,
    pub orientation_sign: i8,
    pub body_id: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurveUse {
    pub population_id: u32,
    pub edge_ordinal: u32,
    pub definition_id: String,
    pub serialized_face_slots: [u32; 2],
    pub body_id: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnresolvedGeometry {
    pub code: DiagnosticCode,
    pub subject_id: Option<String>,
    pub source: Option<StreamSpan>,
    pub native_tag: Option<u32>,
    pub candidates: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeometryModel {
    pub layout: GeometryLayout,
    pub units: UnitContext,
    pub coordinate_space: String,
    pub placement: Frame3,
    pub populations: Vec<GeometryPopulation>,
    pub points: Vec<PointRecord>,
    pub surfaces: Vec<SurfaceRecord>,
    pub curves: Vec<CurveRecord>,
    pub surface_uses: Vec<SurfaceUse>,
    pub curve_uses: Vec<CurveUse>,
    pub unresolved: Vec<UnresolvedGeometry>,
    pub opaque_ranges: Vec<StreamSpan>,
}
impl Default for GeometryModel {
    fn default() -> Self {
        Self {
            layout: GeometryLayout::Unknown,
            units: UnitContext::default(),
            coordinate_space: "world".into(),
            placement: Frame3::identity(),
            populations: Vec::new(),
            points: Vec::new(),
            surfaces: Vec::new(),
            curves: Vec::new(),
            surface_uses: Vec::new(),
            curve_uses: Vec::new(),
            unresolved: Vec::new(),
            opaque_ranges: Vec::new(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeometryDocument {
    pub document: DocumentInfo,
    pub geometry: GeometryModel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topology: Option<crate::topology::TopologyModel>,
}
