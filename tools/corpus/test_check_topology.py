"""Reduction and comparison rules of the topology regression check."""
import unittest

from check_topology import COMPARED_FIELDS, differences, observe


def outcome(topology="complete", faces=2, kind="closed_shell", codes=("container.empty_declaration_unverified",)):
    return {
        "report": {
            "schema_version": 2,
            "status": "partial",
            "features": {"container": "partial", "metadata": "partial", "geometry": "partial", "topology": topology},
            "diagnostics": [{"code": code, "severity": "warning", "message": "m", "location": None, "details": {}} for code in codes],
        },
        "value": {
            "topology": {
                "native_uv": {"parameter_maps": [1, 2, 3], "pcurves": [], "runs": [1]},
                "bodies": [{"id": "b", "kind": kind, "regions": ["r0"], "export_eligible": kind == "closed_shell", "solid_validated": False}],
                "faces": list(range(faces)),
                "edges": list(range(faces * 2)),
                "unresolved": [],
            }
        },
    }


class ObserveTests(unittest.TestCase):
    def test_counts_cover_every_list_and_bodies_keep_their_kind(self):
        observed = observe(outcome())
        self.assertEqual(observed["counts"], {"bodies": 1, "faces": 2, "edges": 4, "unresolved": 0})
        self.assertEqual(observed["native_uv_counts"], {"parameter_maps": 3, "pcurves": 0, "runs": 1})
        self.assertEqual(observed["bodies"], [{"kind": "closed_shell", "regions": 1, "export_eligible": True, "solid_validated": False}])
        self.assertEqual(observed["diagnostics"], ["container.empty_declaration_unverified"])
        self.assertEqual(observed["features"]["topology"], "complete")
        self.assertEqual(set(observed), set(COMPARED_FIELDS))

    def test_missing_document_reduces_to_empty_populations(self):
        observed = observe({"report": {"status": "rejected", "features": {"topology": "invalid"}, "diagnostics": []}, "value": None})
        self.assertEqual(observed["counts"], {})
        self.assertEqual(observed["bodies"], [])
        self.assertEqual(observed["diagnostics"], [])


class DifferenceTests(unittest.TestCase):
    def test_identical_observations_have_no_differences(self):
        self.assertEqual(differences(observe(outcome()), observe(outcome())), [])

    def test_each_changed_field_is_named_once(self):
        expected = observe(outcome())
        observed = observe(outcome(topology="partial", faces=1, kind="open_shell", codes=("topology.geometry_unsupported",)))
        fields = [d["field"] for d in differences(expected, observed)]
        self.assertEqual(fields, ["features", "counts", "bodies", "diagnostics"])
        self.assertEqual(differences(expected, observed)[0]["expected"]["topology"], "complete")

    def test_diagnostic_order_and_multiplicity_do_not_matter(self):
        a = observe(outcome(codes=("x.one", "x.two", "x.one")))
        b = observe(outcome(codes=("x.two", "x.one")))
        self.assertEqual(differences(a, b), [])


if __name__ == "__main__":
    unittest.main()
