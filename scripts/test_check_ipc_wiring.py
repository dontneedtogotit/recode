"""Regression checks for the static frontend/native IPC boundary gate."""
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

CHECKER = Path(__file__).with_name("check-ipc-wiring.py")


class IpcWiringTests(unittest.TestCase):
    def check_fixture(self, frontend: str, native: str, command: str = "save_plan"):
        with tempfile.TemporaryDirectory(prefix="recode-ipc-wiring-") as temp:
            root = Path(temp)
            (root / "scripts").mkdir()
            (root / "src/lib").mkdir(parents=True)
            (root / "src-tauri/src").mkdir(parents=True)
            shutil.copyfile(CHECKER, root / "scripts/check-ipc-wiring.py")
            (root / "src/lib/ipc.ts").write_text(frontend)
            (root / "src-tauri/src/main.rs").write_text(
                f"tauri::generate_handler![recode::tauri_commands::{command},]"
            )
            (root / "src-tauri/src/tauri_commands.rs").write_text(native)
            return subprocess.run(
                ["python3", str(root / "scripts/check-ipc-wiring.py")],
                text=True, capture_output=True,
            )

    def test_rejects_flat_payload_for_nested_native_input(self):
        result = self.check_fixture(
            "invokeCommand<void>('save_plan', { id: 'one', goal: 'ship' });",
            "pub fn save_plan(input: SavePlanInput) -> Result<(), String> { Ok(()) }",
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("save_plan", result.stderr)
        self.assertIn("input", result.stderr)

    def test_accepts_nested_payload_across_lines(self):
        result = self.check_fixture(
            "invokeCommand<{ id: string }>(\n  'save_plan',\n  { input: { id: 'one', goal: 'ship' } },\n);",
            "pub fn save_plan(input: SavePlanInput) -> Result<(), String> { Ok(()) }",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("1 frontend commands", result.stdout)

    def test_rejects_default_camel_case_argument_mismatch(self):
        result = self.check_fixture(
            "invokeCommand<boolean>('get_status', { run_id: 'one' });",
            "#[tauri::command]\npub fn get_status(run_id: String) -> bool { true }",
            command="get_status",
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("runId", result.stderr)

    def test_accepts_explicit_snake_case_argument(self):
        result = self.check_fixture(
            "invokeCommand<boolean>('get_status', { run_id: 'one' });",
            "#[tauri::command(rename_all = \"snake_case\")]\npub fn get_status(run_id: String) -> bool { true }",
            command="get_status",
        )
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
