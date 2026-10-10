#!/usr/bin/env python3
"""Check literal frontend IPC names and top-level argument keys against Tauri handlers."""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
IPC = ROOT / "src/lib/ipc.ts"
MAIN = ROOT / "src-tauri/src/main.rs"


def skip_space(text: str, pos: int) -> int:
    while pos < len(text) and text[pos].isspace():
        pos += 1
    return pos


def matching(text: str, pos: int, opening: str, closing: str) -> int:
    depth = 0
    quote = None
    for i in range(pos, len(text)):
        char = text[i]
        if quote:
            if char == quote and (i == 0 or text[i - 1] != "\\"):
                quote = None
        elif char in "'\"`":
            quote = char
        elif char == opening:
            depth += 1
        elif char == closing:
            depth -= 1
            if depth == 0:
                return i
    raise ValueError(f"Unbalanced {opening} in IPC call or native signature")


def object_keys(text: str, start: int) -> set[str]:
    """Read only top-level keys of a TypeScript object literal, including shorthands."""
    end = matching(text, start, "{", "}")
    keys = set()
    depth = 0
    quote = None
    expect_key = True
    i = start
    while i <= end:
        char = text[i]
        if quote:
            if char == quote and (i == 0 or text[i - 1] != "\\"):
                quote = None
        elif char in "'\"`":
            quote = char
        elif char in "{[()":
            depth += 1
        elif char in "}])":
            depth -= 1
        elif char == "," and depth == 1:
            expect_key = True
        elif depth == 1 and expect_key and (char.isalpha() or char in "_$"):
            match = re.match(r"[A-Za-z_$][\w$]*", text[i:])
            assert match
            name = match.group(0)
            following = skip_space(text, i + len(name))
            if following <= end and text[following] in ":,}":
                keys.add(name)
                expect_key = False
            i += len(name) - 1
        i += 1
    return keys


def frontend_calls(frontend: str) -> dict[str, set[str]]:
    calls = {}
    for found in re.finditer(r"\binvokeCommand\b", frontend):
        pos = skip_space(frontend, found.end())
        if pos < len(frontend) and frontend[pos] == "<":
            pos = skip_space(frontend, matching(frontend, pos, "<", ">") + 1)
        if pos >= len(frontend) or frontend[pos] != "(":
            continue
        pos = skip_space(frontend, pos + 1)
        if pos >= len(frontend) or frontend[pos] not in "'\"":
            continue  # The function declaration has a variable here, not a command.
        quote = frontend[pos]
        end = frontend.find(quote, pos + 1)
        if end < 0:
            raise ValueError("Unterminated IPC command string")
        name = frontend[pos + 1:end]
        pos = skip_space(frontend, end + 1)
        if pos < len(frontend) and frontend[pos] == ",":
            pos = skip_space(frontend, pos + 1)
        keys = object_keys(frontend, pos) if pos < len(frontend) and frontend[pos] == "{" else set()
        calls.setdefault(name, set()).update(keys)
    return calls


def rust_args(module: str, name: str) -> set[str]:
    base = ROOT / "src-tauri/src" / module.replace("::", "/")
    source = base.with_suffix(".rs")
    if not source.exists():
        source = base / "mod.rs"
    if not source.exists():
        raise ValueError(f"Registered handler source missing: {module}::{name}")
    code = source.read_text()
    found = re.search(rf"\bpub\s+(?:async\s+)?fn\s+{re.escape(name)}\s*\(", code)
    if not found:
        raise ValueError(f"Registered handler definition missing: {module}::{name}")
    begin = found.end() - 1
    signature = code[begin + 1:matching(code, begin, "(", ")")]
    # Multi-line parameters may contain generic types, but only parameter names
    # directly following '(' or ',' at top level are collected.
    args = set()
    depth = 0
    start = 0
    for i, char in enumerate(signature + ","):
        if char in "<([{":
            depth += 1
        elif char in ">)]}":
            depth -= 1
        elif char == "," and depth == 0:
            param = signature[start:i].strip()
            match = re.match(r"([a-z_][\w]*)\s*:\s*(.+)", param, re.S)
            if match and "tauri::AppHandle" not in match.group(2):
                args.add(match.group(1))
            start = i + 1
    before = code[max(0, found.start() - 130):found.start()]
    if not re.search(r'#\[tauri::command\(rename_all\s*=\s*"snake_case"\)\]\s*$', before):
        args = {re.sub(r"_([a-z])", lambda m: m.group(1).upper(), arg) for arg in args}
    return args


def main() -> int:
    frontend = IPC.read_text()
    backend = MAIN.read_text()
    calls = frontend_calls(frontend)
    if "tauri::generate_handler![" not in backend:
        print("IPC gate failed: native handler registration not found", file=sys.stderr)
        return 1
    handler = backend.split("tauri::generate_handler![", 1)[1].split("]", 1)[0]
    registered = {name: module for module, name in re.findall(r"\brecode::([\w:]+)::(\w+)\s*,", handler)}
    if not calls or not registered:
        print("IPC gate failed: could not read frontend calls or registered handlers", file=sys.stderr)
        return 1
    missing = sorted(calls.keys() - registered.keys())
    if missing:
        print("IPC gate failed: unregistered command(s): " + ", ".join(missing), file=sys.stderr)
        return 1
    mismatches = []
    for name, keys in calls.items():
        required = rust_args(registered[name], name)
        absent = required - keys
        if absent:
            mismatches.append(f"{name} missing {', '.join(sorted(absent))}")
    if mismatches:
        print("IPC gate failed: argument mismatch: " + "; ".join(mismatches), file=sys.stderr)
        return 1
    print(f"IPC wiring: {len(calls)} frontend commands registered with native argument keys")
    return 0


if __name__ == "__main__":
    sys.exit(main())
