"""Source-level and contract-level FilmTron branding, security, and GemiMCP parity guards."""
from pathlib import Path
import json
import unittest

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "apps" / "filmcraft"
BRAND = "FilmTron"
UPSTREAM = "FilmCraft"


class BrandingContract(unittest.TestCase):
    def test_full_upstream_source_present(self):
        for path in ("Cargo.toml", "Cargo.lock", "LICENSE-MIT", "LICENSE-APACHE"):
            with self.subTest(path=path):
                self.assertTrue((ROOT / path).is_file(), f"Missing original fork source/license: {path}")
        self.assertTrue((APP / "Cargo.toml").is_file())

    def test_native_window_and_windows_metadata(self):
        main = (APP / "src" / "main.rs").read_text(encoding="utf-8")
        build = (APP / "build.rs").read_text(encoding="utf-8")
        self.assertIn(f'"{BRAND}"', main, "Native window identity must show Tron brand")
        self.assertNotIn(f'.with_title("{UPSTREAM}")', main)
        self.assertNotIn(f'run_native(\n        "{UPSTREAM}"', main)
        self.assertIn(f'.set("ProductName", "{BRAND}")', build)
        self.assertIn(f'.set("FileDescription", "{BRAND} ', build)
        self.assertIn("LegalCopyright", build, "Fork must retain original legal attribution")
        self.assertIn('OriginalFilename", "filmcraft.exe"', build,
                      "Native executable/installer ID migration requires compatibility tests")

    def test_inferno_pixel_glass_themes_and_typography(self):
        theme_rs = (ROOT / "crates" / "ui-egui" / "src" / "theme.rs").read_text(encoding="utf-8")
        fonts_rs = (ROOT / "crates" / "text" / "src" / "fonts.rs").read_text(encoding="utf-8")
        self.assertIn("InfernoDark", theme_rs)
        self.assertIn("InfernoLight", theme_rs)
        self.assertIn("Inferno Pixel Glass (Dark)", theme_rs)
        self.assertIn("Inferno Pixel Glass (Light)", theme_rs)
        self.assertIn("inferno_dark", theme_rs)
        self.assertIn("inferno_light", theme_rs)
        self.assertIn("GemiFontSans", fonts_rs)

    def test_security_gemimcp_and_gemiguard_contracts(self):
        security_rs = (ROOT / "crates" / "automation" / "src" / "security.rs").read_text(encoding="utf-8")
        gemimcp_rs = (ROOT / "crates" / "automation" / "src" / "gemimcp.rs").read_text(encoding="utf-8")
        gemiguard_rs = (ROOT / "crates" / "automation" / "src" / "gemiguard.rs").read_text(encoding="utf-8")
        self.assertIn("authentication required", security_rs)
        self.assertIn("REVISION_CONFLICT", security_rs)
        self.assertIn("IDEMPOTENCY_CONFLICT", security_rs)
        self.assertIn("AuthorizedWorkspace", security_rs)
        self.assertIn("OutboundBridgeAdapter", gemimcp_rs)
        self.assertIn("ed25519_verify", gemiguard_rs)
        self.assertIn("PkceChallenge", gemiguard_rs)

        manifest_path = ROOT / "contracts" / "gemimcp-product-filmtron.v1.json"
        self.assertTrue(manifest_path.is_file(), "Missing GemiMCP product manifest")
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        self.assertEqual(manifest["productId"], "filmtron")
        self.assertTrue(manifest["federation"]["outboundOnly"])
        self.assertTrue(manifest["federation"]["noPublicLocalListeners"])
        self.assertGreaterEqual(manifest["semanticToolCount"], 30)
        self.assertGreaterEqual(manifest["engineCommandToolCount"], 280)

    def test_product_contract_still_exists(self):
        self.assertTrue((ROOT / "TODO.md").is_file())
        self.assertTrue((ROOT / "docs" / "01-FORK-PRODUCT-AND-CLOUD-MCP-CHARTER.md").is_file())
        self.assertTrue((ROOT / "docs" / "implementation" / "07-NATIVE-CONTROL-AND-MCP-SECURITY-AUDIT.md").is_file())
        self.assertTrue((ROOT / "docs" / "implementation" / "08-SOURCE-COMPLETENESS-AND-PARITY-MATRIX.md").is_file())
        self.assertTrue((ROOT / "scripts" / "import-upstream.sh").is_file())


if __name__ == "__main__":
    unittest.main()
