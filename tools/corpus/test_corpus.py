"""Adversarial acquisition and pairing checks for the corpus tooling."""
import io
from pathlib import Path
import tempfile
import unittest
from zipfile import ZipFile, ZipInfo

from corpus import case_info, check_archive, header_inventory, pair_candidates, selected_members, step_inventory


class CorpusTests(unittest.TestCase):
    def test_zip_path_escape_rejected_even_for_unselected_member(self):
        for name in ["../escape.exe", "/absolute.pdf", "C:/drive.pdf", "a\\escape.pdf"]:
            with self.subTest(name=name):
                data = io.BytesIO()
                with ZipFile(data, "w") as z:
                    z.writestr(name, b"data")
                with ZipFile(data) as z, self.assertRaises(ValueError):
                    list(selected_members(z))

    def test_zip_symlink_and_duplicate_destination_rejected(self):
        link = ZipInfo("symlink.CATPart")
        link.external_attr = 0o120777 << 16
        data = io.BytesIO()
        with ZipFile(data, "w") as z:
            z.writestr(link, "../target")
        with ZipFile(data) as z, self.assertRaises(ValueError):
            list(selected_members(z))
        data = io.BytesIO()
        with ZipFile(data, "w") as z:
            z.writestr("a/model.CATPart", b"one")
            z.writestr("a/./model.CATPart", b"two")
        with ZipFile(data) as z, self.assertRaises(ValueError):
            list(selected_members(z))

    def test_same_length_archive_tampering_fails_hash(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "model.zip"
            path.write_bytes(b"tampered")
            with self.assertRaises(ValueError):
                check_archive(path, {"size_bytes": 8, "sha256": "0" * 64})

    def test_tessellation_and_revision_mismatch_not_preferred(self):
        native = {**case_info("nist_ftc_08_asme1_rc_ct5290_ra.CATPart")}
        steps = []
        for name in ["nist_ftc_08_asme1_rc.stp", "nist_ftc_08_asme1_rd.stp", "nist_ftc_08_asme1_ap242-e1-tg.stp"]:
            steps.append({**case_info(name), "path": "Definitions/" + name,
                          "sha256": "0" * 64, "step": {"tessellation_only_candidate": False}})
        pairs = pair_candidates(native, steps)
        self.assertEqual(sum(p["preferred_geometry_reference"] for p in pairs), 1)
        self.assertEqual({p["status"] for p in pairs}, {"same_case_and_revision_unverified",
                         "case_matches_revision_differs", "tessellated_visualization_only"})
        self.assertEqual(case_info("nist_ftc_06_asme1_ct5315_rd.CATPart")["revision"], "rd")

    def test_misleading_nested_magic_not_accepted(self):
        blob = b"V5_CFV2\0" + bytes(8) + b"noiseV5_CFV2\0" + bytes(16)
        result = header_inventory(blob)
        self.assertFalse(result["directory_ends_at_eof"])
        self.assertFalse(result["nested_candidates"][0]["directory_valid"])
        self.assertEqual(header_inventory(b"short")["magic_valid"], False)

    def test_step_counts_and_exact_data_hash_handle_non_ascii_header(self):
        data = b"ISO-10303-21;HEADER;/*\xff*/FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));ENDSEC;DATA;#1=PLANE('',#2);ENDSEC;END-ISO-10303-21;"
        result = step_inventory(data)
        self.assertEqual(result["schema"], ["CONFIG_CONTROL_DESIGN"])
        self.assertEqual(result["entity_counts"], {"PLANE": 1})
        self.assertEqual(len(result["data_section_sha256"]), 64)


if __name__ == "__main__":
    unittest.main()
