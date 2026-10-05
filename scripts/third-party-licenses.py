#!/usr/bin/env python3
"""Copy resolved dependency notices verbatim for native CLI distribution."""

import argparse
import hashlib
import json
import pathlib
import shutil
import subprocess
import sys
import urllib.parse


ROOT = pathlib.Path(__file__).resolve().parents[1]
DESTINATION = ROOT / "licenses" / "third-party"
TARGETS = (
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
)
PREFIXES = ("license", "licence", "copying", "copyright", "notice")


def dependencies(offline=False):
    packages = {}
    for target in TARGETS:
        command = [
            "cargo", "metadata", "--locked", "--format-version", "1",
            "--features", "cli", "--filter-platform", target,
        ]
        if offline:
            command.append("--offline")
        metadata = json.loads(subprocess.check_output(command, cwd=ROOT))
        all_packages = {package["id"]: package for package in metadata["packages"]}
        nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
        root = metadata["resolve"]["root"]
        queue = [root]
        visited = set()
        while queue:
            identity = queue.pop()
            if identity in visited:
                continue
            visited.add(identity)
            node = nodes[identity]
            if identity != root:
                entry = packages.setdefault(identity, {
                    "package": all_packages[identity],
                    "targets": set(),
                    "features": set(),
                })
                entry["targets"].add(target)
                entry["features"].update(node["features"])
            for dependency in node["deps"]:
                # Include normal and build dependencies conservatively; exclude
                # test-only edges, which are not a shipped CLI feature surface.
                if any(kind["kind"] != "dev" for kind in dependency["dep_kinds"]):
                    queue.append(dependency["pkg"])
    return packages


def notice_files(package):
    directory = pathlib.Path(package["manifest_path"]).parent
    found = set()
    declared = package.get("license_file")
    if declared:
        file = pathlib.Path(declared)
        if not file.is_absolute():
            file = directory / file
        if file.is_file() and file.is_relative_to(directory):
            found.add(file)
    for file in directory.rglob("*"):
        relative = file.relative_to(directory)
        name_matches = file.name.lower().startswith(PREFIXES)
        directory_matches = any(
            component.lower() in ("license", "licenses", "licence", "licences")
            for component in relative.parts[:-1]
        )
        if (name_matches or directory_matches) and file.is_file():
            found.add(file)
    return directory, sorted(found)


def rendered_files(offline=False):
    packages = dependencies(offline)
    overrides = json.loads((ROOT / "licenses/upstream/overrides.json").read_text())
    expected = {}
    missing = []
    supplemental = {}
    rows = []
    folders = set()
    for identity, entry in sorted(packages.items(), key=lambda item: (
        item[1]["package"]["name"], item[1]["package"]["version"], item[0],
    )):
        package = entry["package"]
        name, version = package["name"], package["version"]
        folder = name + "-" + version
        if folder in folders:
            folder += "-" + hashlib.sha256(identity.encode()).hexdigest()[:8]
        folders.add(folder)
        directory, files = notice_files(package)
        links = []
        for file in files:
            relative = pathlib.PurePosixPath(folder, file.relative_to(directory).as_posix())
            expected[relative.as_posix()] = file.read_bytes()
            links.append("[" + file.relative_to(directory).as_posix() + "](" +
                         urllib.parse.quote(relative.as_posix()) + ")")
        override = overrides.get(name + "@" + version) if not files else None
        if override:
            vcs = json.loads((directory / ".cargo_vcs_info.json").read_text())
            if vcs["git"]["sha1"] != override["vcs_sha1"]:
                raise ValueError("Supplemental notice VCS revision mismatch: " + name)
            for notice in override["files"]:
                source_file = (ROOT / notice["path"]).resolve()
                if not source_file.is_relative_to((ROOT / "licenses/upstream").resolve()):
                    raise ValueError("Supplemental notice escaped its source directory")
                content = source_file.read_bytes()
                if hashlib.sha256(content).hexdigest() != notice["sha256"]:
                    raise ValueError("Supplemental notice bytes changed: " + notice["path"])
                if notice.get("source_path"):
                    canonical = (ROOT / notice["source_path"]).resolve()
                    if not canonical.is_relative_to((ROOT / "licenses/upstream").resolve()):
                        raise ValueError("Canonical notice escaped its source directory")
                    original = canonical.read_bytes()
                    if hashlib.sha256(original).hexdigest() != notice["source_sha256"]:
                        raise ValueError("Canonical notice source bytes changed: " + notice["source_path"])
                    header = b"MIT License\n\nCopyright (c) <year> <copyright holders>\n\n"
                    if notice["selection"] != "permission-and-disclaimer" or not original.startswith(header):
                        raise ValueError("Unsupported canonical notice selection")
                    if content != original[len(header):]:
                        raise ValueError("Canonical MIT permission/disclaimer selection changed")
                relative = pathlib.PurePosixPath(folder, notice["name"])
                expected[relative.as_posix()] = content
                links.append("[" + notice["name"] + "](" +
                             urllib.parse.quote(relative.as_posix()) + ")")
            supplemental[name + "@" + version] = override
        if not files and (not override or override.get("notice_text_unavailable")):
            missing.append({
                "name": name, "version": version,
                "declared_license": package.get("license"),
                "repository": package.get("repository"),
                "canonical_terms_supplied": bool(override and override.get("canonical_terms_supplied")),
                "upstream_copyright_notice_unavailable": bool(
                    override and override.get("upstream_copyright_notice_unavailable")
                ),
            })
        source = "https://crates.io/api/v1/crates/" + name + "/" + version + "/download"
        source_label = "[exact source package](" + source + ")"
        if not str(package.get("source", "")).startswith("registry+"):
            source_label = package.get("repository") or "Non-registry source: inspect Cargo.lock"
        rows.append("| " + " | ".join((
            name, version, package.get("license") or "Unspecified upstream",
            (", ".join(links) + (" (recorded supplement)" if override else ""))
            if links else "**No notice file in the resolved source package**",
            source_label,
        )) + " |")
    readme = "\n".join([
        "# Third-party license and copyright texts", "",
        "Generated by `python3 scripts/third-party-licenses.py` from locked Cargo",
        "metadata with the optional CLI feature for the five native release targets.",
        "Normal and build dependency edges are included conservatively; test-only",
        "edges are excluded. Files preserve original bytes from the resolved Cargo",
        "source package or recorded supplemental upstream/canonical sources,",
        "including available copyright notices and disclaimers.",
        "Dependencies keep their own licenses; the project's MIT OR Apache-2.0",
        "license applies to original project code, not these copied texts.", "",
        "Exact source-package links provide the unmodified covered dependency",
        "sources, including MPL-2.0 dependencies. The feature/target inventory is",
        "recorded in `inventory.json`, along with exact supplemental notice URLs,",
        "VCS revisions, selections, and SHA-256 hashes. Regenerate when Cargo.lock",
        "or CLI features change, and include this tree in native binary archives.", "",
        "For dependencies whose exact upstream sources omit notice files, recorded",
        "supplements preserve the original Cargo license/authors declarations and",
        "include canonical MIT permission and disclaimer terms. The canonical text",
        "is pinned by SPDX repository commit and SHA-256, with a byte-checked excerpt",
        "that excludes its replaceable copyright template. No upstream copyright",
        "notice was supplied for these dependencies; no holder or year is invented.", "",
        "| Dependency | Version | Upstream declaration | Included notices | Source |",
        "| --- | --- | --- | --- | --- |",
        *rows, "", "## Missing upstream notice files", "", "",
    ])
    if missing:
        readme += "\n".join(
            "- `" + package["name"] + " " + package["version"] + "`: " +
            (package["declared_license"] or "license unspecified") +
            ("; no upstream copyright notice was supplied in the Cargo package or "
             "recorded exact upstream repository revision. The original Cargo "
             "license/authors declaration and canonical MIT permission/disclaimer "
             "terms are included separately. No copyright holder or year is invented."
             if package["canonical_terms_supplied"] else
             "; full license/copyright notice text was unavailable in the Cargo "
             "package and recorded exact upstream repository revision. The original "
             "Cargo license/authors declaration is preserved separately. Inspect "
             "the source and upstream repository before distributing it; an SPDX "
             "declaration is not a replacement for missing notice text.")
            for package in missing
        ) + "\n"
    else:
        readme += "Every included dependency has at least one copied upstream notice file.\n"
    expected["README.md"] = readme.encode()
    inventory = {
        "schema_version": 1,
        "feature": "cli",
        "targets": list(TARGETS),
        "dependencies": [{
            "name": entry["package"]["name"],
            "version": entry["package"]["version"],
            "source": entry["package"].get("source"),
            "license": entry["package"].get("license"),
            "features": sorted(entry["features"]),
            "targets": sorted(entry["targets"]),
        } for _, entry in sorted(packages.items())],
        "missing_notice_files": missing,
        "supplemental_notices": supplemental,
    }
    expected["inventory.json"] = (json.dumps(inventory, indent=2) + "\n").encode()
    return expected, missing, len(packages)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify generated notices without rewriting them")
    parser.add_argument("--offline", action="store_true", help="resolve locked metadata only from the local Cargo cache")
    args = parser.parse_args()
    expected, missing, package_count = rendered_files(args.offline)
    if args.check:
        actual = {
            file.relative_to(DESTINATION).as_posix(): file.read_bytes()
            for file in DESTINATION.rglob("*") if file.is_file()
        }
        if actual != expected:
            print("Third-party notices differ; run python3 scripts/third-party-licenses.py", file=sys.stderr)
            return 1
    else:
        if DESTINATION.exists():
            shutil.rmtree(DESTINATION)
        DESTINATION.mkdir(parents=True)
        for name, content in expected.items():
            file = DESTINATION / name
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_bytes(content)
    print(json.dumps({
        "dependencies": package_count,
        "copied_notice_files": len(expected) - 2,
        "missing_notice_files": missing,
        "checked": args.check,
    }, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
