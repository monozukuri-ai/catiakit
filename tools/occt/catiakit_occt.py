#!/usr/bin/env python3
"""Build an Open CASCADE shape from a catiakit `decode --topology` outcome.

The adapter transfers the decoded B-rep as it is: shared vertices and edges,
the decoded loops with their coedge directions, and the decoded parametric
curves. Planes, cylinders, cones, tori and NURBS use the same parameterization
in catiakit and in the kernel, so their pcurves are transferred directly.
Spherical faces are decoded in a stereographic chart; the adapter expresses
them in a kernel sphere whose poles and seam lie outside the decoded
hemisphere and derives their pcurves from the sampled 3D edges. Procedural
carriers (rolling-ball surfaces, surface jets, cylinder-cylinder
intersections) are interpolated from the samples written by the
`sample_geometry` example with the decoded parameters, so that the kernel
parameterization follows the decoded one. Polar connectors become degenerated
edges.

No kernel repair runs on the transferred data. Validity is checked as
transferred; an optional repaired copy is produced separately and reported as
such. Requires the `OCP` Python bindings (cadquery-ocp). This is a development
adapter, not a runtime dependency of catiakit.
"""
from __future__ import annotations

import math
from dataclasses import dataclass, field

from OCP.BRep import BRep_Builder, BRep_Tool
from OCP.Approx import Approx_IsoParametric
from OCP.BRepBndLib import BRepBndLib
from OCP.BRepBuilderAPI import BRepBuilderAPI_MakeEdge, BRepBuilderAPI_MakeSolid, BRepBuilderAPI_MakeVertex
from OCP.BRepCheck import BRepCheck_Analyzer
from OCP.BRepGProp import BRepGProp
from OCP.BRepLib import BRepLib
from OCP.Bnd import Bnd_Box
from OCP.Geom import (
    Geom_BSplineCurve,
    Geom_BSplineSurface,
    Geom_Circle,
    Geom_ConicalSurface,
    Geom_CylindricalSurface,
    Geom_Ellipse,
    Geom_Line,
    Geom_Plane,
    Geom_SphericalSurface,
    Geom_ToroidalSurface,
)
from OCP.Geom2d import Geom2d_Line
from OCP.Geom2dAPI import Geom2dAPI_Interpolate
from OCP.GeomAbs import GeomAbs_C2
from OCP.GeomAPI import GeomAPI_Interpolate, GeomAPI_PointsToBSplineSurface
from OCP.GProp import GProp_GProps
from OCP.gp import gp_Ax2, gp_Ax3, gp_Dir, gp_Dir2d, gp_Pnt, gp_Pnt2d
from OCP.ShapeAnalysis import ShapeAnalysis_FreeBounds
from OCP.ShapeFix import ShapeFix_Shape
from OCP.TColgp import TColgp_Array1OfPnt, TColgp_Array2OfPnt, TColgp_HArray1OfPnt, TColgp_HArray1OfPnt2d
from OCP.TColStd import TColStd_Array1OfInteger, TColStd_Array1OfReal, TColStd_Array2OfReal, TColStd_HArray1OfReal
from OCP.TopAbs import TopAbs_EDGE, TopAbs_FACE, TopAbs_FORWARD, TopAbs_REVERSED, TopAbs_SHELL, TopAbs_SOLID, TopAbs_VERTEX
from OCP.TopExp import TopExp_Explorer
from OCP.TopoDS import TopoDS_Compound, TopoDS, TopoDS_Edge, TopoDS_Face, TopoDS_Shell, TopoDS_Wire

TAU = 2.0 * math.pi


class AdapterError(ValueError):
    pass


def _pnt(p):
    return gp_Pnt(float(p[0]), float(p[1]), float(p[2]))


def _dir(v):
    return gp_Dir(float(v[0]), float(v[1]), float(v[2]))


def _ax3(origin, z_axis, x_axis):
    return gp_Ax3(_pnt(origin), _dir(z_axis), _dir(x_axis))


def _ax2(frame):
    return gp_Ax2(_pnt(frame["origin"]), _dir(frame["z_axis"]), _dir(frame["x_axis"]))


def _knot_vector(knots):
    """Full knot vector to kernel unique knots and multiplicities."""
    unique, mults = [], []
    for k in knots:
        if unique and k == unique[-1]:
            mults[-1] += 1
        else:
            unique.append(k)
            mults.append(1)
    ku = TColStd_Array1OfReal(1, len(unique))
    km = TColStd_Array1OfInteger(1, len(unique))
    for i, (k, m) in enumerate(zip(unique, mults), start=1):
        ku.SetValue(i, float(k))
        km.SetValue(i, int(m))
    return ku, km


def _merge_bezier_junctions(points, weights, knots, degree, tolerance=1.0e-7):
    """Reduce interior knots of multiplicity degree+1 to degree.

    Multi-arc curves are stored with independent polynomial arcs that share
    their junction pole; the kernel accepts interior multiplicities up to the
    degree only. Dropping the duplicated pole is exact when the arcs meet at
    the same pole, with the following arc's weights rescaled to the junction
    weight (a uniform weight scale leaves a rational arc unchanged).
    """
    unique, mults = [], []
    for k in knots:
        if unique and k == unique[-1]:
            mults[-1] += 1
        else:
            unique.append(k)
            mults.append(1)
    points = [list(p) for p in points]
    weights = [float(w) for w in weights]
    drop = set()
    first = 0  # full-vector index of the current unique knot
    for i, multiplicity in enumerate(mults):
        if 0 < i < len(unique) - 1 and multiplicity > degree:
            if multiplicity > degree + 1:
                raise AdapterError(f"interior knot multiplicity {multiplicity} exceeds degree+1")
            a, b = first - 1, first
            if math.dist(points[a], points[b]) > tolerance:
                raise AdapterError(f"multi-arc curve is discontinuous at knot {unique[i]} ({math.dist(points[a], points[b]):.3e} mm)")
            factor = weights[a] / weights[b]
            if factor != 1.0:
                for j in range(b, len(weights)):
                    weights[j] *= factor
            drop.add(b)
            mults[i] = degree
        first += multiplicity
    kept = [j for j in range(len(points)) if j not in drop]
    return [points[j] for j in kept], [weights[j] for j in kept], unique, mults, len(drop)


def _merge_surface_junctions(poles, weights, knots, degree, tolerance=1.0e-7):
    """Row-wise counterpart of `_merge_bezier_junctions` for a pole grid.

    `poles[i][j]` and `weights[i][j]` are indexed by the merged direction
    first; a whole row is dropped at each interior knot of multiplicity
    degree+1 when it coincides with the previous row.
    """
    unique, mults = [], []
    for k in knots:
        if unique and k == unique[-1]:
            mults[-1] += 1
        else:
            unique.append(k)
            mults.append(1)
    poles = [[list(p) for p in row] for row in poles]
    weights = [[float(w) for w in row] for row in weights]
    drop = set()
    first = 0
    for i, multiplicity in enumerate(mults):
        if 0 < i < len(unique) - 1 and multiplicity > degree:
            if multiplicity > degree + 1:
                raise AdapterError(f"interior knot multiplicity {multiplicity} exceeds degree+1")
            a, b = first - 1, first
            gap = max(math.dist(pa, pb) for pa, pb in zip(poles[a], poles[b]))
            if gap > tolerance:
                raise AdapterError(f"multi-arc surface is discontinuous at knot {unique[i]} ({gap:.3e} mm)")
            factors = {wa / wb for wa, wb in zip(weights[a], weights[b])}
            if len(factors) != 1:
                raise AdapterError(f"multi-arc surface junction at knot {unique[i]} has non-proportional weights")
            factor = factors.pop()
            if factor != 1.0:
                for r in range(b, len(weights)):
                    weights[r] = [w * factor for w in weights[r]]
            drop.add(b)
            mults[i] = degree
        first += multiplicity
    kept = [r for r in range(len(poles)) if r not in drop]
    return [poles[r] for r in kept], [weights[r] for r in kept], unique, mults, len(drop)


def _integer_array(values):
    array = TColStd_Array1OfInteger(1, len(values))
    for i, v in enumerate(values, start=1):
        array.SetValue(i, int(v))
    return array


def _rescale_knots(surface, u_range, v_range):
    """Map the knot vectors of a B-spline surface affinely onto the given ranges."""
    for count, read, write, (lo, hi) in (
        (surface.NbUKnots(), surface.UKnots, surface.SetUKnots, u_range),
        (surface.NbVKnots(), surface.VKnots, surface.SetVKnots, v_range),
    ):
        knots = TColStd_Array1OfReal(1, count)
        read(knots)
        k0, k1 = knots(1), knots(count)
        for i in range(1, count + 1):
            knots.SetValue(i, lo + (hi - lo) * (knots(i) - k0) / (k1 - k0))
        write(knots)


def _real_array(values):
    array = TColStd_Array1OfReal(1, len(values))
    for i, v in enumerate(values, start=1):
        array.SetValue(i, float(v))
    return array


def interpolate_3d(points, parameters, tolerance=1.0e-9):
    """C2 B-spline through `points` at the given strictly increasing parameters."""
    array = TColgp_HArray1OfPnt(1, len(points))
    params = TColStd_HArray1OfReal(1, len(points))
    for i, (p, t) in enumerate(zip(points, parameters), start=1):
        array.SetValue(i, _pnt(p))
        params.SetValue(i, float(t))
    builder = GeomAPI_Interpolate(array, params, False, tolerance)
    builder.Perform()
    if not builder.IsDone():
        raise AdapterError("3D interpolation failed")
    return builder.Curve()


def interpolate_2d(points, parameters, tolerance=1.0e-9):
    array = TColgp_HArray1OfPnt2d(1, len(points))
    params = TColStd_HArray1OfReal(1, len(points))
    for i, (p, t) in enumerate(zip(points, parameters), start=1):
        array.SetValue(i, gp_Pnt2d(float(p[0]), float(p[1])))
        params.SetValue(i, float(t))
    builder = Geom2dAPI_Interpolate(array, params, False, tolerance)
    builder.Perform()
    if not builder.IsDone():
        raise AdapterError("2D interpolation failed")
    return builder.Curve()


def _monotone(points, parameters):
    """Drop repeated parameters and return strictly increasing sequences."""
    if parameters[0] > parameters[-1]:
        points, parameters = points[::-1], parameters[::-1]
    out_p, out_t = [points[0]], [parameters[0]]
    for p, t in zip(points[1:], parameters[1:]):
        if t > out_t[-1]:
            out_p.append(p)
            out_t.append(t)
    if len(out_t) < 2:
        raise AdapterError("fewer than two distinct parameters")
    return out_p, out_t


@dataclass
class KernelSurface:
    surface: object
    chart: str  # "native" (same parameterization as catiakit) or "sphere_rotated"
    frame: dict | None = None
    radius: float = 0.0
    note: str | None = None


def make_surface(record, grids):
    g = record["geometry"]
    kind = g["kind"]
    f = g.get("frame")
    if kind == "plane":
        return KernelSurface(Geom_Plane(_ax3(f["origin"], f["z_axis"], f["x_axis"])), "native")
    if kind == "cylinder":
        return KernelSurface(Geom_CylindricalSurface(_ax3(f["origin"], f["z_axis"], f["x_axis"]), float(g["radius"])), "native")
    if kind == "cone":
        # Both parameterizations place the apex at v = 0 with the same half angle.
        return KernelSurface(Geom_ConicalSurface(_ax3(f["origin"], f["z_axis"], f["x_axis"]), float(g["half_angle"]), 0.0), "native")
    if kind == "sphere":
        return KernelSurface(Geom_SphericalSurface(_ax3(f["origin"], f["z_axis"], f["x_axis"]), float(g["radius"])), "native")
    if kind == "sphere_stereographic":
        # The decoded face lies in the closed +Z hemisphere of its frame. A
        # kernel sphere with poles at +-X and seam in the -Z hemisphere keeps
        # both singularities outside a face that stays away from the equator
        # points +-X: Z' = X, X' = -Z, Y' = Y. Faces whose boundary reaches
        # those points (hemispherical caps) are rebuilt around the +Z pole
        # with a seam and a degenerated edge; see `polar_sphere`.
        z_axis = f["x_axis"]
        x_axis = [-c for c in f["z_axis"]]
        return KernelSurface(
            Geom_SphericalSurface(_ax3(f["origin"], z_axis, x_axis), float(g["radius"])),
            "sphere_rotated",
            frame={"origin": f["origin"], "x_axis": x_axis, "y_axis": f["y_axis"], "z_axis": z_axis},
            radius=float(g["radius"]),
            note="kernel sphere axis rotated away from the decoded hemisphere",
        )
    if kind == "torus":
        return KernelSurface(Geom_ToroidalSurface(_ax3(f["origin"], f["z_axis"], f["x_axis"]), float(g["major_radius"]), float(g["minor_radius"])), "native")
    if kind == "nurbs":
        s = g["surface"]
        nu, nv = s["control_counts"]
        du, dv = (int(d) for d in s["degrees"])
        grid = [[s["control_points"][i * nv + j] for j in range(nv)] for i in range(nu)]
        scalars = [[s["weights"][i * nv + j] for j in range(nv)] for i in range(nu)]
        grid, scalars, u_knots, u_mults, merged_u = _merge_surface_junctions(grid, scalars, s["knots"][0], du)
        transposed_grid = [list(column) for column in zip(*grid)]
        transposed_scalars = [list(column) for column in zip(*scalars)]
        transposed_grid, transposed_scalars, v_knots, v_mults, merged_v = _merge_surface_junctions(transposed_grid, transposed_scalars, s["knots"][1], dv)
        grid = [list(row) for row in zip(*transposed_grid)]
        scalars = [list(row) for row in zip(*transposed_scalars)]
        nu, nv = len(grid), len(grid[0])
        poles = TColgp_Array2OfPnt(1, nu, 1, nv)
        weights = TColStd_Array2OfReal(1, nu, 1, nv)
        for i in range(nu):
            for j in range(nv):
                poles.SetValue(i + 1, j + 1, _pnt(grid[i][j]))
                weights.SetValue(i + 1, j + 1, float(scalars[i][j]))
        surface = Geom_BSplineSurface(poles, weights, _real_array(u_knots), _real_array(v_knots), _integer_array(u_mults), _integer_array(v_mults), du, dv)
        note = f"multi-arc: {merged_u + merged_v} shared junction pole rows merged exactly" if merged_u or merged_v else None
        return KernelSurface(surface, "native", note=note)
    if kind == "rolling_ball":
        grid = grids.get(record["id"])
        if not grid or not grid["points"]:
            raise AdapterError(f"no samples for rolling-ball surface {record['id']}")
        rows = grid["points"]
        nu, nv = len(rows), len(rows[0])
        array = TColgp_Array2OfPnt(1, nu, 1, nv)
        for i, row in enumerate(rows, start=1):
            for j, p in enumerate(row, start=1):
                array.SetValue(i, j, _pnt(p))
        u0, u1 = grid["u_range"]
        v0, v1 = grid["v_range"]
        # The grid is uniform in the decoded parameters, so the uniformly
        # parameterized cubic interpolant maps to the decoded ranges by an
        # affine change of its knots, which leaves the surface unchanged.
        fit = GeomAPI_PointsToBSplineSurface()
        fit.Interpolate(array, Approx_IsoParametric)
        if not fit.IsDone():
            raise AdapterError(f"rolling-ball surface {record['id']} did not interpolate")
        surface = fit.Surface()
        _rescale_knots(surface, (u0, u1), (v0, v1))
        return KernelSurface(surface, "native", note=f"cubic interpolant through a {nu}x{nv} sample grid with the decoded parameters")
    raise AdapterError(f"unsupported surface kind {kind} for {record['id']}")


def make_curve(record, edge, edge_samples):
    """Return `(Geom_Curve, shift, note)`; `shift` maps decoded to kernel parameters."""
    g = record["geometry"]
    kind = g["kind"]
    if kind == "line":
        return Geom_Line(_pnt(g["origin"]), _dir(g["direction"])), 0.0, None
    if kind == "circle":
        return Geom_Circle(_ax2(g["frame"]), float(g["radius"])), 0.0, None
    if kind == "ellipse":
        a, b = float(g["major_radius"]), float(g["minor_radius"])
        f = g["frame"]
        if a >= b:
            return Geom_Ellipse(_ax2(f), a, b), 0.0, None
        # Kernel ellipses need the major axis first: rotate the frame by a
        # quarter turn (x' = y, y' = -x) and shift the parameter accordingly.
        swapped = {"origin": f["origin"], "x_axis": f["y_axis"], "z_axis": f["z_axis"]}
        return Geom_Ellipse(_ax2(swapped), b, a), -math.pi / 2.0, None
    if kind == "nurbs":
        c = g["curve"]
        degree = int(c["degree"])
        points, scalars, unique, mults, merged = _merge_bezier_junctions(c["control_points"], c["weights"], c["knots"], degree)
        n = len(points)
        poles = TColgp_Array1OfPnt(1, n)
        weights = TColStd_Array1OfReal(1, n)
        for i, (p, w) in enumerate(zip(points, scalars), start=1):
            poles.SetValue(i, _pnt(p))
            weights.SetValue(i, w)
        knots = _real_array(unique)
        multiplicities = TColStd_Array1OfInteger(1, len(mults))
        for i, m in enumerate(mults, start=1):
            multiplicities.SetValue(i, int(m))
        note = f"multi-arc: {merged} shared junction poles merged exactly" if merged else None
        return Geom_BSplineCurve(poles, weights, knots, multiplicities, degree), 0.0, note
    if kind in ("cylinder_intersection", "surface_jet", "rolling_intersection"):
        if not edge_samples or not edge_samples["points"]:
            raise AdapterError(f"no samples for {kind} curve {record['id']}")
        points = edge_samples["points"]
        a, b = edge["parameter_range"]
        parameters = [a + (b - a) * i / (len(points) - 1) for i in range(len(points))]
        points, parameters = _monotone(points, parameters)
        return interpolate_3d(points, parameters), 0.0, f"interpolated through {len(points)} samples with the decoded parameters"
    if kind == "point":
        raise AdapterError(f"degenerate curve {record['id']} is not built as an edge")
    raise AdapterError(f"unsupported curve kind {kind} for {record['id']}")


def sphere_uv(kernel, p):
    """Rotated-chart parameters of a 3D point on a stereographic sphere face."""
    f = kernel.frame
    o = f["origin"]
    d = [p[i] - o[i] for i in range(3)]
    x = sum(d[i] * f["x_axis"][i] for i in range(3))
    y = sum(d[i] * f["y_axis"][i] for i in range(3))
    z = sum(d[i] * f["z_axis"][i] for i in range(3))
    u = math.atan2(y, x)
    v = math.asin(max(-1.0, min(1.0, z / kernel.radius)))
    return [u, v]


def _unit(v):
    n = math.sqrt(sum(c * c for c in v))
    return [c / n for c in v]


def _cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def rotated_sphere(record, z_axis, x_axis):
    """Kernel sphere with the given axis; the seam meridian lies at x_axis."""
    g = record["geometry"]
    f = g["frame"]
    z_axis = _unit(z_axis)
    x_axis = _unit([x_axis[i] - sum(x_axis[j] * z_axis[j] for j in range(3)) * z_axis[i] for i in range(3)])
    y_axis = _cross(z_axis, x_axis)
    return KernelSurface(
        Geom_SphericalSurface(_ax3(f["origin"], z_axis, x_axis), float(g["radius"])),
        "sphere_rotated",
        frame={"origin": f["origin"], "x_axis": x_axis, "y_axis": y_axis, "z_axis": z_axis},
        radius=float(g["radius"]),
        note="kernel sphere axis chosen so that both poles and the seam stay outside the face",
    )


def sphere_chart_candidates(record):
    """Axes whose poles may avoid the decoded face, with the seam in -Z."""
    f = record["geometry"]["frame"]
    x, y, z = f["x_axis"], f["y_axis"], f["z_axis"]
    minus_z = [-c for c in z]
    s = math.sqrt(0.5)
    equatorial = [x, y, [-c for c in x], [-c for c in y],
                  [s * (x[i] + y[i]) for i in range(3)], [s * (x[i] - y[i]) for i in range(3)],
                  [s * (-x[i] + y[i]) for i in range(3)], [s * (-x[i] - y[i]) for i in range(3)]]
    for axis in equatorial:
        yield rotated_sphere(record, axis, minus_z)
    # Axes tilted into the -Z hemisphere: one pole is certainly outside the
    # face and the other sits at 45 degrees elevation on the opposite side.
    for axis in equatorial:
        tilted = [s * (axis[i] - z[i]) for i in range(3)]
        yield rotated_sphere(record, tilted, minus_z)


def polar_sphere(record, first_point):
    """Kernel sphere around the decoded +Z axis with the seam through `first_point`."""
    g = record["geometry"]
    f = g["frame"]
    o = f["origin"]
    d = [first_point[i] - o[i] for i in range(3)]
    z = f["z_axis"]
    height = sum(d[i] * z[i] for i in range(3))
    radial = [d[i] - height * z[i] for i in range(3)]
    if math.sqrt(sum(c * c for c in radial)) < 1.0e-9 * float(g["radius"]):
        radial = f["x_axis"]
    x_axis = _unit(radial)
    y_axis = _cross(z, x_axis)
    return KernelSurface(
        Geom_SphericalSurface(_ax3(o, z, x_axis), float(g["radius"])),
        "sphere_polar",
        frame={"origin": o, "x_axis": x_axis, "y_axis": y_axis, "z_axis": z},
        radius=float(g["radius"]),
        note="kernel sphere around the decoded axis with a synthesized seam and polar degenerated edge",
    )


def unwrap(sequence, period=TAU):
    """Continue the first coordinate so consecutive values never jump by half a period."""
    out = []
    for i, (u, v) in enumerate(sequence):
        if i:
            previous = out[-1][0]
            while u - previous > period / 2.0:
                u -= period
            while previous - u > period / 2.0:
                u += period
        out.append([u, v])
    return out


def signed_area(polygon):
    area = 0.0
    for (x0, y0), (x1, y1) in zip(polygon, polygon[1:] + polygon[:1]):
        area += x0 * y1 - x1 * y0
    return area / 2.0


@dataclass
class BuildReport:
    faces_built: int = 0
    faces_failed: list = field(default_factory=list)
    faces_reversed: int = 0
    edges_built: int = 0
    degenerated_edges: int = 0
    seam_edges: int = 0
    approximated_surfaces: dict = field(default_factory=dict)
    approximated_curves: dict = field(default_factory=dict)
    multi_arc_curves: int = 0
    multi_arc_surfaces: int = 0
    rotated_sphere_faces: int = 0
    polar_sphere_faces: int = 0
    max_vertex_gap: float = 0.0
    max_pcurve_deviation: float = 0.0
    max_edge_tolerance: float = 0.0
    raw_faces_valid: int = 0
    raw_faces_invalid: int = 0
    free_edges: int | None = None
    shell_closed: bool | None = None
    solid_valid: bool | None = None
    repaired: bool = False
    repaired_solid_valid: bool | None = None
    face_debug: list = field(default_factory=list)


def build_shape(outcome, samples, precision=1.0e-3):
    """Build the first body of a complete decode outcome.

    Returns `(shape, repaired_shape_or_None, report)`. The first shape carries
    the decoded data without kernel repairs. A repaired copy is produced only
    when the transferred solid is invalid.
    """
    value = outcome["value"]
    geometry, topology = value["geometry"], value["topology"]
    report = BuildReport()
    surfaces = {s["id"]: s for s in geometry["surfaces"]}
    curves = {c["id"]: c for c in geometry["curves"]}
    points = {p["id"]: p for p in geometry["points"]}
    edge_samples = {e["id"]: e for e in samples["edges"]}
    pcurve_samples = {p["coedge"]: p for p in samples["pcurves"]}
    grids = {g["id"]: g for g in samples["surface_grids"]}
    builder = BRep_Builder()

    vertices = {v["id"]: BRepBuilderAPI_MakeVertex(_pnt(points[v["point"]]["position"])).Vertex() for v in topology["vertices"]}

    exact_curves = {}
    edges = {}
    edge_forward = {}
    edge_shift = {}
    edge_ranges = {}
    for e in topology["edges"]:
        if e["degenerate"]:
            continue
        cid = e["curve"]
        kind = curves[cid]["geometry"]["kind"]
        if kind in ("cylinder_intersection", "surface_jet", "rolling_intersection"):
            curve, shift, note = make_curve(curves[cid], e, edge_samples.get(e["id"]))
            report.approximated_curves[e["id"]] = f"{kind}: {note}"
        else:
            if cid not in exact_curves:
                exact_curves[cid] = make_curve(curves[cid], e, None)
                if exact_curves[cid][2] and exact_curves[cid][2].startswith("multi-arc"):
                    report.multi_arc_curves += 1
            curve, shift, _ = exact_curves[cid]
        a, b = (float(x) for x in e["parameter_range"])
        lo, hi = a + shift, b + shift
        forward = lo <= hi
        v0, v1 = vertices[e["vertices"][0]], vertices[e["vertices"][1]]
        if not forward:
            lo, hi, v0, v1 = hi, lo, v1, v0
        for vertex, parameter in ((v0, lo), (v1, hi)):
            gap = BRep_Tool.Pnt_s(vertex).Distance(curve.Value(parameter))
            if gap > BRep_Tool.Tolerance_s(vertex):
                builder.UpdateVertex(vertex, gap * 1.000001 + 1.0e-9)
            report.max_vertex_gap = max(report.max_vertex_gap, gap)
        maker = BRepBuilderAPI_MakeEdge(curve, v0, v1, lo, hi)
        if not maker.IsDone():
            raise AdapterError(f"edge {e['id']} on curve {cid} could not be built (status {maker.Error()})")
        edge = maker.Edge()
        # The kernel moves parameters of periodic curves into the base period;
        # the pcurves below must use the same parameter labels as the edge.
        kernel_lo, kernel_hi = BRep_Tool.Range_s(edge)
        if abs((kernel_hi - kernel_lo) - (hi - lo)) > 1.0e-9:
            raise AdapterError(f"edge {e['id']} range changed from {hi - lo} to {kernel_hi - kernel_lo}")
        edges[e["id"]] = edge
        edge_forward[e["id"]] = forward
        edge_shift[e["id"]] = shift + (kernel_lo - lo)
        edge_ranges[e["id"]] = (kernel_lo, kernel_hi)
        report.edges_built += 1

    coedges = {c["id"]: c for c in topology["coedges"]}
    loops = {l["id"]: l for l in topology["loops"]}
    edge_records = {e["id"]: e for e in topology["edges"]}
    faces = []
    for f in topology["faces"]:
        try:
            kernel = make_surface(surfaces[f["surface"]], grids)
            if kernel.note and kernel.chart == "native":
                if kernel.note.startswith("multi-arc"):
                    report.multi_arc_surfaces += 1
                else:
                    report.approximated_surfaces[f["surface"]] = kernel.note
            if kernel.chart == "sphere_rotated":
                # Pick an axis whose poles are neither on the boundary nor
                # enclosed by the outer loop. A boundary that reaches every
                # candidate pole (a hemispherical cap bounded by the equator)
                # is rebuilt around the decoded +Z pole with a seam instead.
                chosen = None
                for candidate in sphere_chart_candidates(surfaces[f["surface"]]):
                    acceptable = True
                    for loop_id in f["loops"]:
                        loop = loops[loop_id]
                        sequence = []
                        for coedge_id in loop["coedges"]:
                            coedge = coedges[coedge_id]
                            pts = edge_samples[coedge["edge"]]["points"]
                            sequence.extend(sphere_uv(candidate, p) for p in (pts if coedge["forward"] else pts[::-1]))
                        if any(abs(uv[1]) > math.pi / 2.0 - 1.0e-3 for uv in sequence):
                            acceptable = False
                            break
                        flat = unwrap(sequence)
                        if max(uv[0] for uv in flat) - min(uv[0] for uv in flat) > TAU - 1.0e-3 or abs(flat[-1][0] - flat[0][0]) > 1.0e-3:
                            acceptable = False
                            break
                    if acceptable:
                        chosen = candidate
                        break
                if chosen is not None:
                    kernel = chosen
                    report.rotated_sphere_faces += 1
                else:
                    outer = next(loops[l] for l in f["loops"] if loops[l]["role"] == "outer")
                    first_coedge = coedges[outer["coedges"][0]]
                    pts = edge_samples[first_coedge["edge"]]["points"]
                    kernel = polar_sphere(surfaces[f["surface"]], pts[0] if first_coedge["forward"] else pts[-1])
                    report.polar_sphere_faces += 1
            face = TopoDS_Face()
            builder.MakeFace(face, kernel.surface, 1.0e-7)
            face_pcurves = {}  # edge id -> {TopAbs orientation: pcurve}
            wires = []
            outer_polygon = None
            loop_sequences = []
            for loop_id in f["loops"]:
                loop = loops[loop_id]
                sequences = []
                for coedge_id in loop["coedges"]:
                    coedge = coedges[coedge_id]
                    edge_id = coedge["edge"]
                    if edge_id not in edges:
                        raise AdapterError(f"coedge {coedge_id} uses an unbuilt edge {edge_id}")
                    forward = coedge["forward"] == edge_forward[edge_id]
                    sample = pcurve_samples.get(coedge_id)
                    if sample is None or not sample["uv"]:
                        raise AdapterError(f"no pcurve samples for coedge {coedge_id}")
                    r0, r1 = sample["edge_parameter_range"]
                    n = len(sample["uv"])
                    shift = edge_shift[edge_id]
                    parameters = [r0 + (r1 - r0) * i / (n - 1) + shift for i in range(n)]
                    if kernel.chart in ("sphere_rotated", "sphere_polar"):
                        points3d = edge_samples[edge_id]["points"]
                        if len(points3d) != n:
                            raise AdapterError("edge and pcurve sample counts differ")
                        ordered = points3d if coedge["forward"] else points3d[::-1]
                        uv = [sphere_uv(kernel, p) for p in ordered]
                    else:
                        uv = sample["uv"]
                    sequences.append({"coedge": coedge, "edge_id": edge_id, "forward": forward, "uv": uv, "parameters": parameters, "points3d": (edge_samples[edge_id]["points"] if coedge["forward"] else edge_samples[edge_id]["points"][::-1])})
                if kernel.chart == "sphere_rotated":
                    # One continuous unwrapped loop in the rotated chart; the
                    # face never reaches the seam, so the loop stays connected.
                    flat = unwrap([uv for s in sequences for uv in s["uv"]])
                    at = 0
                    for s in sequences:
                        n = len(s["uv"])
                        s["uv"] = [list(uv) for uv in flat[at:at + n]]
                        at += n
                if kernel.chart == "sphere_polar":
                    # The loop winds once around the +Z pole, starting at the
                    # seam (u = 0). Points at the pole itself have no angle;
                    # carry the previous angle through them.
                    flat = [uv for s in sequences for uv in s["uv"]]
                    flat[0] = [0.0, flat[0][1]]
                    flat = unwrap(flat)
                    at = 0
                    for s in sequences:
                        n = len(s["uv"])
                        s["uv"] = [list(uv) for uv in flat[at:at + n]]
                        at += n
                    if loop["role"] == "outer":
                        turn = flat[-1][0] - flat[0][0]
                        if abs(abs(turn) - TAU) > 1.0e-3:
                            raise AdapterError(f"polar sphere loop winds {turn:.4f} rad instead of one turn")
                        loop["_polar_turn"] = turn
                loop_sequences.append((loop, sequences))
            # Periodic charts: move the whole face into the base period, using
            # the outer loop as the reference so every loop keeps its relation.
            periodic_u = surfaces[f["surface"]]["geometry"]["kind"] in ("cylinder", "cone", "sphere", "sphere_stereographic", "torus")
            periodic_v = surfaces[f["surface"]]["geometry"]["kind"] == "torus"
            if periodic_u or periodic_v:
                reference = next((seq for loop, seq in loop_sequences if loop["role"] == "outer"), loop_sequences[0][1] if loop_sequences else [])
                flat = [uv for s in reference for uv in s["uv"]]
                if flat:
                    mean_u = sum(uv[0] for uv in flat) / len(flat)
                    mean_v = sum(uv[1] for uv in flat) / len(flat)
                    du = -math.floor(mean_u / TAU) * TAU if periodic_u else 0.0
                    dv = -math.floor(mean_v / TAU) * TAU if periodic_v else 0.0
                    if du or dv:
                        for loop, sequences in loop_sequences:
                            for s in sequences:
                                s["uv"] = [[u + du, v + dv] for u, v in s["uv"]]
                            for transition in loop.get("singular_transitions", []):
                                transition["uv_start"] = [transition["uv_start"][0] + du, transition["uv_start"][1] + dv]
                                transition["uv_end"] = [transition["uv_end"][0] + du, transition["uv_end"][1] + dv]
            for loop, sequences in loop_sequences:
                polygon = [uv for s in sequences for uv in s["uv"][:-1]]
                transitions = {t["after_coedge"]: t for t in loop.get("singular_transitions", [])}
                wire = TopoDS_Wire()
                builder.MakeWire(wire)
                for s in sequences:
                    edge = edges[s["edge_id"]]
                    pts, prm = _monotone(s["uv"], s["parameters"])
                    pcurve = interpolate_2d(pts, prm)
                    orientation = TopAbs_FORWARD if s["forward"] else TopAbs_REVERSED
                    face_pcurves.setdefault(s["edge_id"], {})[orientation] = pcurve
                    # Deviation between the kernel surface through the pcurve and the decoded 3D samples.
                    deviation = 0.0
                    for (u, v), p in zip(s["uv"], s["points3d"]):
                        deviation = max(deviation, kernel.surface.Value(u, v).Distance(_pnt(p)))
                    report.max_pcurve_deviation = max(report.max_pcurve_deviation, deviation)
                    if deviation > BRep_Tool.Tolerance_s(edge):
                        builder.UpdateEdge(edge, deviation * 1.000001 + 1.0e-9)
                    builder.Add(wire, TopoDS.Edge_s(edge.Oriented(orientation)))
                    transition = transitions.get(s["coedge"]["id"])
                    if transition is not None:
                        if kernel.chart == "sphere_rotated":
                            raise AdapterError("polar connector on a rotated sphere chart")
                        start, end = transition["uv_start"], transition["uv_end"]
                        delta = [end[0] - start[0], end[1] - start[1]]
                        length = math.hypot(*delta)
                        if length <= 0.0:
                            continue
                        connector = TopoDS_Edge()
                        builder.MakeEdge(connector)
                        builder.Degenerated(connector, True)
                        line = Geom2d_Line(gp_Pnt2d(*start), gp_Dir2d(*delta))
                        builder.UpdateEdge(connector, line, face, 1.0e-7)
                        builder.Range(connector, face, 0.0, length)
                        apex = vertices[transition["vertex"]]
                        start_vertex = TopoDS.Vertex_s(apex.Oriented(TopAbs_FORWARD))
                        end_vertex = TopoDS.Vertex_s(apex.Oriented(TopAbs_REVERSED))
                        builder.Add(connector, start_vertex)
                        builder.Add(connector, end_vertex)
                        builder.UpdateVertex(start_vertex, 0.0, connector, face, 1.0e-7)
                        builder.UpdateVertex(end_vertex, length, connector, face, 1.0e-7)
                        builder.Add(wire, connector)
                        polygon.append(list(end))
                        report.degenerated_edges += 1
                if kernel.chart == "sphere_polar" and loop["role"] == "outer":
                    # Close the cap with a seam up to the pole, the polar
                    # degenerated edge, and the seam back down. The seam is one
                    # edge with two pcurves (u = turn and u = 0).
                    turn = loop["_polar_turn"]
                    polar = math.pi / 2.0
                    first = sequences[0]["uv"][0]
                    last = sequences[-1]["uv"][-1]
                    meridian = kernel.surface.UIso(0.0)
                    start_point = kernel.surface.Value(0.0, first[1])
                    pole_point = kernel.surface.Value(0.0, polar)
                    first_coedge = sequences[0]["coedge"]
                    first_edge = edge_records[first_coedge["edge"]]
                    seam_start = vertices[first_edge["vertices"][0 if first_coedge["forward"] else 1]]
                    gap = BRep_Tool.Pnt_s(seam_start).Distance(start_point)
                    if gap > BRep_Tool.Tolerance_s(seam_start):
                        builder.UpdateVertex(seam_start, gap * 1.000001 + 1.0e-9)
                    pole_vertex = BRepBuilderAPI_MakeVertex(pole_point).Vertex()
                    lo, hi = first[1], polar
                    seam = BRepBuilderAPI_MakeEdge(meridian, seam_start, pole_vertex, lo, hi).Edge()
                    # Seam traversed upward (forward) on the side where the loop
                    # ended and downward (reversed) where it started.
                    up = interpolate_2d([[last[0], lo], [last[0], hi]], [lo, hi])
                    down = interpolate_2d([[first[0], lo], [first[0], hi]], [lo, hi])
                    builder.UpdateEdge(seam, up, down, face, 1.0e-7)
                    upward, downward = TopoDS.Edge_s(seam.Oriented(TopAbs_FORWARD)), TopoDS.Edge_s(seam.Oriented(TopAbs_REVERSED))
                    connector = TopoDS_Edge()
                    builder.MakeEdge(connector)
                    builder.Degenerated(connector, True)
                    length = abs(turn)
                    direction = -1.0 if turn > 0 else 1.0
                    line = Geom2d_Line(gp_Pnt2d(last[0], polar), gp_Dir2d(direction, 0.0))
                    builder.UpdateEdge(connector, line, face, 1.0e-7)
                    builder.Range(connector, face, 0.0, length)
                    c0 = TopoDS.Vertex_s(pole_vertex.Oriented(TopAbs_FORWARD))
                    c1 = TopoDS.Vertex_s(pole_vertex.Oriented(TopAbs_REVERSED))
                    builder.Add(connector, c0)
                    builder.Add(connector, c1)
                    builder.UpdateVertex(c0, 0.0, connector, face, 1.0e-7)
                    builder.UpdateVertex(c1, length, connector, face, 1.0e-7)
                    builder.Add(wire, upward)
                    builder.Add(wire, connector)
                    builder.Add(wire, downward)
                    report.seam_edges += 1
                    report.degenerated_edges += 1
                    polygon = polygon + [[last[0], polar], [first[0], polar]]
                wires.append(wire)
                if loop["role"] == "outer":
                    outer_polygon = polygon
            for edge_id, by_orientation in face_pcurves.items():
                edge = edges[edge_id]
                if len(by_orientation) == 2:
                    builder.UpdateEdge(edge, by_orientation[TopAbs_FORWARD], by_orientation[TopAbs_REVERSED], face, BRep_Tool.Tolerance_s(edge))
                    report.seam_edges += 1
                else:
                    (pcurve,) = by_orientation.values()
                    builder.UpdateEdge(edge, pcurve, face, BRep_Tool.Tolerance_s(edge))
            reverse = outer_polygon is not None and signed_area(outer_polygon) < 0.0
            for wire in wires:
                builder.Add(face, TopoDS.Wire_s(wire.Reversed()) if reverse else wire)
            if reverse:
                face = TopoDS.Face_s(face.Reversed())
                report.faces_reversed += 1
            report.face_debug.append({"face": f["id"], "surface": f["surface"], "chart": kernel.chart, "outer_polygon_area": signed_area(outer_polygon) if outer_polygon else None, "reversed": reverse, "loops": [{"role": loops[l]["role"], "signed_uv_area": loops[l]["signed_uv_area"], "coedges": len(loops[l]["coedges"])} for l in f["loops"]], "uv_ranges": [[min(uv[0] for s in seq for uv in s["uv"]), max(uv[0] for s in seq for uv in s["uv"]), min(uv[1] for s in seq for uv in s["uv"]), max(uv[1] for s in seq for uv in s["uv"])] for loop, seq in loop_sequences]})
            if BRepCheck_Analyzer(face).IsValid():
                report.raw_faces_valid += 1
            else:
                report.raw_faces_invalid += 1
            faces.append(face)
            report.faces_built += 1
        except (AdapterError, RuntimeError) as error:
            report.faces_failed.append({"face": f["id"], "surface": f["surface"], "error": str(error)})

    shell = TopoDS_Shell()
    builder.MakeShell(shell)
    for face in faces:
        builder.Add(shell, face)
    closed = BRep_Tool.IsClosed_s(shell)
    shell.Closed(closed)
    report.shell_closed = closed
    report.free_edges = free_edge_count(shell, precision)
    shape = shell
    if closed:
        solid = BRepBuilderAPI_MakeSolid(shell).Solid()
        BRepLib.OrientClosedSolid_s(solid)
        shape = solid
        report.solid_valid = BRepCheck_Analyzer(shape).IsValid()
    tolerance = 0.0
    explorer = TopExp_Explorer(shape, TopAbs_EDGE)
    while explorer.More():
        tolerance = max(tolerance, BRep_Tool.Tolerance_s(TopoDS.Edge_s(explorer.Current())))
        explorer.Next()
    report.max_edge_tolerance = tolerance
    repaired = None
    if not (closed and report.solid_valid):
        fixer = ShapeFix_Shape(shape)
        fixer.SetPrecision(precision)
        fixer.Perform()
        repaired = fixer.Shape()
        report.repaired = True
        report.repaired_solid_valid = BRepCheck_Analyzer(repaired).IsValid()
    return shape, repaired, report


def shape_metrics(shape):
    # Fixed-order Gauss integration. The adaptive variant (with a precision
    # argument) split the equal principal moments of an axisymmetric
    # reference by 3e-3 and moved its centroid by 2 um; its results are kept
    # as auxiliary values only.
    volume = GProp_GProps()
    BRepGProp.VolumeProperties_s(shape, volume)
    area = GProp_GProps()
    BRepGProp.SurfaceProperties_s(shape, area)
    adaptive_volume = GProp_GProps()
    BRepGProp.VolumeProperties_s(shape, adaptive_volume, 1.0e-6)
    # The optimal box is evaluated on the geometry itself; the default box
    # encloses control polygons and can exceed a NURBS reference by tens of
    # millimetres.
    box = Bnd_Box()
    BRepBndLib.AddOptimal_s(shape, box, False, False)
    xmin, ymin, zmin, xmax, ymax, zmax = box.Get()
    centre = volume.CentreOfMass()
    counts = {}
    for name, kind in (("solids", TopAbs_SOLID), ("shells", TopAbs_SHELL), ("faces", TopAbs_FACE), ("edges", TopAbs_EDGE), ("vertices", TopAbs_VERTEX)):
        explorer = TopExp_Explorer(shape, kind)
        n = 0
        while explorer.More():
            n += 1
            explorer.Next()
        counts[name] = n
    # Placement-invariant quantities: exporters may re-orient a model.
    extents = sorted([xmax - xmin, ymax - ymin, zmax - zmin])
    box_centre = [(xmin + xmax) / 2.0, (ymin + ymax) / 2.0, (zmin + zmax) / 2.0]
    centroid_offset = math.dist([centre.X(), centre.Y(), centre.Z()], box_centre)
    inertia = volume.PrincipalProperties()
    moments = sorted(inertia.Moments())
    return {
        "volume": volume.Mass(),
        "area": area.Mass(),
        "centroid": [centre.X(), centre.Y(), centre.Z()],
        "bounding_box": [[xmin, ymin, zmin], [xmax, ymax, zmax]],
        "bounding_box_extents_sorted": extents,
        "centroid_offset_from_box_centre": centroid_offset,
        "principal_moments": moments,
        "adaptive_volume": adaptive_volume.Mass(),
        "adaptive_principal_moments": sorted(adaptive_volume.PrincipalProperties().Moments()),
        "counts": counts,
        "valid": BRepCheck_Analyzer(shape).IsValid(),
    }


def free_edge_count(shape, tolerance=1.0e-3):
    analysis = ShapeAnalysis_FreeBounds(shape, tolerance, False, False)
    n = 0
    for compound in (analysis.GetClosedWires(), analysis.GetOpenWires()):
        explorer = TopExp_Explorer(compound, TopAbs_EDGE)
        while explorer.More():
            n += 1
            explorer.Next()
    return n


def read_step(path):
    from OCP.IFSelect import IFSelect_RetDone
    from OCP.STEPControl import STEPControl_Reader

    reader = STEPControl_Reader()
    if reader.ReadFile(str(path)) != IFSelect_RetDone:
        raise AdapterError(f"STEP file could not be read: {path}")
    reader.TransferRoots()
    return solids_only(reader.OneShape())


def solids_only(shape):
    """Keep the solids of a transferred file.

    Files with product and manufacturing information carry annotation curves
    and planes next to the body; they have no volume but widen the bounding
    box. When the file has no solid, the shape is returned as read.
    """
    solids = []
    explorer = TopExp_Explorer(shape, TopAbs_SOLID)
    while explorer.More():
        solids.append(explorer.Current())
        explorer.Next()
    if not solids:
        return shape
    if len(solids) == 1:
        return solids[0]
    compound = TopoDS_Compound()
    builder = BRep_Builder()
    builder.MakeCompound(compound)
    for solid in solids:
        builder.Add(compound, solid)
    return compound


def write_step(shape, path):
    from OCP.IFSelect import IFSelect_RetDone
    from OCP.STEPControl import STEPControl_AsIs, STEPControl_Writer

    writer = STEPControl_Writer()
    writer.Transfer(shape, STEPControl_AsIs)
    if writer.Write(str(path)) != IFSelect_RetDone:
        raise AdapterError(f"STEP file could not be written: {path}")
