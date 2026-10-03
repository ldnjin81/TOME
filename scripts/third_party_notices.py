#!/usr/bin/env python3
"""Writes THIRD-PARTY-NOTICES.md: every crate and npm package that ends up in the Windows
build of TOME, with its license and the license texts it ships (deduplicated).

    python3 scripts/third_party_notices.py [output]   (default: THIRD-PARTY-NOTICES.md)

Run `cargo fetch --target x86_64-pc-windows-msvc` first so Windows-only crates are on disk.
Only normal dependencies count (not build or dev ones); the npm side is what the UI bundle
uses (package-lock entries that are not dev).
"""

import hashlib
import json
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
TARGET = "x86_64-pc-windows-msvc"
LICENSE_FILE = re.compile(r"^(LICEN[CS]E|COPYING|NOTICE|COPYRIGHT|UNLICENSE)([-._].*)?$", re.I)
OWN = {"tome", "tome-core"}


def license_texts(directory: pathlib.Path):
    if not directory.is_dir():
        return []
    files = sorted(p for p in directory.iterdir() if p.is_file() and LICENSE_FILE.match(p.name))
    return [(p.name, p.read_text(encoding="utf-8", errors="replace").strip()) for p in files]


def crates():
    meta = json.loads(subprocess.check_output(["cargo", "metadata", "--format-version", "1", "--filter-platform", TARGET], cwd=ROOT))
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = next(p["id"] for p in meta["packages"] if p["name"] == "tome")
    seen, stack = set(), [root]
    while stack:
        current = stack.pop()
        if current in seen:
            continue
        seen.add(current)
        for dep in nodes[current]["deps"]:
            if any(kind["kind"] is None for kind in dep["dep_kinds"]):
                stack.append(dep["pkg"])
    lore_license = license_texts(ROOT / "third_party" / "lore")
    for package_id in seen:
        p = packages[package_id]
        if p["name"] in OWN:
            continue
        directory = pathlib.Path(p["manifest_path"]).parent
        lore = directory.is_relative_to(ROOT / "third_party" / "lore")
        yield {
            "name": p["name"],
            "version": p["version"],
            "license": p.get("license") or ("MIT" if lore else "see license text"),
            "source": p.get("repository") or ("https://github.com/EpicGames/lore" if lore else f"https://crates.io/crates/{p['name']}"),
            "texts": license_texts(directory) or (lore_license if lore else []),
        }


def npm_packages():
    lock = json.loads((ROOT / "ui" / "package-lock.json").read_text(encoding="utf-8"))
    for path, entry in lock["packages"].items():
        if not path or entry.get("dev") or entry.get("devOptional"):
            continue
        name = path.split("node_modules/")[-1]
        yield {
            "name": name,
            "version": entry.get("version", ""),
            "license": entry.get("license", "see license text"),
            "source": f"https://www.npmjs.com/package/{name}",
            "texts": license_texts(ROOT / "ui" / path),
        }


def main():
    output = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "THIRD-PARTY-NOTICES.md"
    groups = [("Rust crates", sorted(crates(), key=lambda c: (c["name"], c["version"]))), ("npm packages (user interface)", sorted(npm_packages(), key=lambda c: c["name"]))]
    texts = {}
    missing = []
    lines = [
        "# Third-party notices",
        "",
        "TOME includes the software listed below. Each is used under its license; the license texts",
        "follow the list. Lore is by Epic Games, Inc. (MIT). Packages under MPL-2.0 are used",
        "unmodified; their source is at the address given.",
        "",
    ]
    for title, items in groups:
        lines += [f"## {title}", "", "| Package | Version | License | Source |", "|---|---|---|---|"]
        for item in items:
            keys = []
            for name, text in item["texts"]:
                key = hashlib.sha256(text.encode()).hexdigest()[:12]
                texts.setdefault(key, (name, text, []))[2].append(f"{item['name']} {item['version']}")
                keys.append(key)
            if not keys:
                missing.append(item["name"])
            refs = " ".join(f"[{k}](#t-{k})" for k in keys) or "—"
            lines.append(f"| {item['name']} | {item['version']} | {item['license']} {refs} | {item['source']} |")
        lines.append("")
    lines += ["## License texts", ""]
    for key, (name, text, users) in sorted(texts.items(), key=lambda t: t[1][2][0]):
        lines += [f'<a id="t-{key}"></a>', f"### {name} — {', '.join(users)}", "", "```text", text, "```", ""]
    output.write_text("\n".join(lines), encoding="utf-8")
    count = sum(len(items) for _, items in groups)
    print(f"{output}: {count} packages, {len(texts)} distinct license texts")
    if missing:
        print("no license file shipped (license field only):", ", ".join(sorted(set(missing))))


if __name__ == "__main__":
    main()
