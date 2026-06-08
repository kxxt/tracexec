#!/usr/bin/env python3
"""Update UKCI kernel versions in nix/ukci.nix.

The updater intentionally uses kernel.org release metadata and tarball hashes
instead of cloning the Linux repository. Stable and longterm tarball hashes come
from kernel.org sha256sums.asc files. Mainline rc hashes are computed with
nix-prefetch-url because kernel.org exposes rc snapshots from cgit without a
published checksum file.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import urllib.request
from dataclasses import dataclass
from pathlib import Path


KERNEL_RELEASES_URL = "https://www.kernel.org/releases.json"
SHA256SUMS_URL = "https://cdn.kernel.org/pub/linux/kernel/v{major}.x/sha256sums.asc"
UKCI_PATH = Path("nix/ukci.nix")


@dataclass(frozen=True)
class KernelRelease:
    moniker: str
    version: str
    source: str | None


@dataclass(frozen=True)
class KernelUpdate:
    name: str
    tag: str
    version: str | None
    source: str
    sha256: str


@dataclass(frozen=True)
class Entry:
    name: str
    block: str
    start: int
    end: int


@dataclass(frozen=True)
class Region:
    text: str
    start: int
    end: int


def fetch_text(url: str) -> str:
    with urllib.request.urlopen(url, timeout=60) as response:
        return response.read().decode("utf-8")


def load_releases() -> list[KernelRelease]:
    data = json.loads(fetch_text(KERNEL_RELEASES_URL))
    releases = []
    for release in data["releases"]:
        releases.append(
            KernelRelease(
                moniker=release["moniker"],
                version=release["version"],
                source=release.get("source"),
            )
        )
    return releases


def version_key(version: str) -> tuple[int, int, int, int]:
    match = re.fullmatch(r"(\d+)\.(\d+)(?:\.(\d+))?(?:-rc(\d+))?", version)
    if not match:
        raise ValueError(f"Unsupported kernel version: {version}")
    major, minor, patch, rc = match.groups()
    return (int(major), int(minor), int(patch or 0), int(rc or 0))


def branch_name(version: str) -> str:
    major, minor, *_ = version.split(".", 2)
    return f"{major}.{minor.split('-', 1)[0]}"


def rc_nix_version(version: str) -> str:
    major_minor, rc = version.split("-rc", 1)
    return f"{major_minor}.0-rc{rc}"


def major_version(version: str) -> str:
    return version.removeprefix("v").split(".", 1)[0]


def sha_file_name(version: str) -> str:
    return f"linux-{version}.tar.xz"


def fetch_kernel_org_hash(version: str, cache: dict[str, dict[str, str]]) -> str:
    major = major_version(version)
    if major not in cache:
        sums = {}
        for line in fetch_text(SHA256SUMS_URL.format(major=major)).splitlines():
            parts = line.split()
            if len(parts) == 2 and re.fullmatch(r"[0-9a-f]{64}", parts[0]):
                sums[parts[1]] = parts[0]
        cache[major] = sums

    filename = sha_file_name(version)
    try:
        base16_hash = cache[major][filename]
    except KeyError as exc:
        raise RuntimeError(
            f"Could not find {filename} in kernel.org sha256 sums for v{major}.x"
        ) from exc

    return convert_hash_to_sri(base16_hash)


def convert_hash_to_sri(hash_value: str) -> str:
    result = subprocess.run(
        [
            "nix",
            "hash",
            "convert",
            "--hash-algo",
            "sha256",
            "--to",
            "sri",
            hash_value,
        ],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    )
    return result.stdout.strip()


def prefetch_rc_hash(source_url: str, version: str) -> str:
    filename = f"linux-{version}.tar.gz"
    result = subprocess.run(
        [
            "nix-prefetch-url",
            "--type",
            "sha256",
            "--name",
            filename,
            source_url,
        ],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    )
    base32_hash = result.stdout.strip().splitlines()[-1]
    return convert_hash_to_sri(base32_hash)


def find_release(releases: list[KernelRelease], moniker: str) -> KernelRelease:
    matches = [release for release in releases if release.moniker == moniker]
    if not matches:
        raise RuntimeError(f"kernel.org did not report a {moniker} release")
    return max(matches, key=lambda release: version_key(release.version))


def find_longterm_release(
    releases: list[KernelRelease], branch: str
) -> KernelRelease:
    matches = [
        release
        for release in releases
        if release.moniker == "longterm" and branch_name(release.version) == branch
    ]
    if not matches:
        raise RuntimeError(f"kernel.org did not report longterm branch {branch}")
    return max(matches, key=lambda release: version_key(release.version))


def find_sources_region(text: str) -> Region:
    sources_match = re.search(r"(?m)^[^\S\n]*sourcesFor[^\S\n]*=", text)
    if sources_match is None:
        raise RuntimeError("Could not locate UKCI sourcesFor definition")

    targets_match = re.search(
        r"(?m)^[^\S\n]*sourcesForTargets[^\S\n]*=",
        text[sources_match.end() :],
    )
    if targets_match is None:
        raise RuntimeError("Could not locate UKCI sourcesForTargets definition")

    start = sources_match.start()
    end = sources_match.end() + targets_match.start()
    return Region(text=text[start:end], start=start, end=end)


def split_entries(text: str, offset: int = 0) -> list[Entry]:
    entries: list[Entry] = []
    stack: list[int] = []
    in_comment = False
    in_string = False
    escaped = False

    for index, char in enumerate(text):
        if in_comment:
            if char == "\n":
                in_comment = False
            continue

        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            continue

        if char == "#":
            in_comment = True
        elif char == '"':
            in_string = True
        elif char == "{":
            stack.append(index)
        elif char == "}" and stack:
            start = stack.pop()
            end = index + 1
            block = text[start:end]
            name = field_value(block, "name")
            if name is not None:
                entries.append(
                    Entry(name=name, block=block, start=offset + start, end=offset + end)
                )

    return sorted(entries, key=lambda entry: entry.start)


def source_entries(text: str) -> list[Entry]:
    region = find_sources_region(text)
    return split_entries(region.text, offset=region.start)


def field_value(block: str, field: str) -> str | None:
    match = re.search(
        rf'(?m)^[^\S\n]*{re.escape(field)}[^\S\n]*=[^\S\n]*"([^"]*)"[^\S\n]*;',
        block,
    )
    if match is None:
        return None
    return match.group(1)


def update_field(block: str, field: str, value: str) -> str:
    updated, count = re.subn(
        rf'(?m)^([^\S\n]*{re.escape(field)}[^\S\n]*=[^\S\n]*)"[^"]*"([^\S\n]*;)',
        rf'\1"{value}"\2',
        block,
        count=1,
    )
    if count != 1:
        raise RuntimeError(f"Could not update {field} in block:\n{block}")
    return updated


def set_optional_field(block: str, field: str, value: str | None) -> str:
    pattern = re.compile(
        rf'(?m)^[^\S\n]*{re.escape(field)}[^\S\n]*=[^\S\n]*"[^"]*"[^\S\n]*;\n?'
    )
    if value is None:
        return pattern.sub("", block, count=1)
    if pattern.search(block):
        return update_field(block, field, value)

    tag_line = re.search(
        r'(?m)^([^\S\n]*)tag[^\S\n]*=[^\S\n]*"[^"]*"[^\S\n]*;\n?',
        block,
    )
    if tag_line is None:
        raise RuntimeError(f"Could not insert {field} into block:\n{block}")
    insert_at = tag_line.end()
    indent = tag_line.group(1)
    return block[:insert_at] + f'{indent}{field} = "{value}";\n' + block[insert_at:]


def update_block(block: str, update: KernelUpdate) -> str:
    updated = update_field(block, "name", update.name)
    updated = update_field(updated, "tag", update.tag)
    updated = set_optional_field(updated, "version", update.version)
    updated = update_field(updated, "source", update.source)
    updated = update_field(updated, "sha256", update.sha256)
    return updated


def apply_updates(text: str, updates: dict[str, KernelUpdate]) -> str:
    entries = source_entries(text)
    replacements: list[tuple[int, int, str]] = []
    used_updates: set[str] = set()

    for entry in entries:
        update = updates.get(entry.name)
        if update is None:
            continue
        replacements.append((entry.start, entry.end, update_block(entry.block, update)))
        used_updates.add(entry.name)

    missing = set(updates) - used_updates
    if missing:
        raise RuntimeError(f"Could not find UKCI entries: {', '.join(sorted(missing))}")

    for start, end, replacement in reversed(replacements):
        text = text[:start] + replacement + text[end:]
    return text


def build_updates(
    current_text: str, releases: list[KernelRelease], update_rc: bool
) -> dict[str, KernelUpdate]:
    hash_cache: dict[str, dict[str, str]] = {}
    updates: dict[str, KernelUpdate] = {}
    entries = source_entries(current_text)

    lts_names = sorted(
        {
            entry.name
            for entry in entries
            if re.fullmatch(r"\d+\.\d+lts", entry.name)
        },
        key=lambda name: version_key(name.removesuffix("lts")),
    )
    for name in lts_names:
        branch = name.removesuffix("lts")
        release = find_longterm_release(releases, branch)
        updates[name] = KernelUpdate(
            name=name,
            tag=release.version,
            version=None,
            source="mirror" if major_version(release.version) == "6" else "kernel-org",
            sha256=fetch_kernel_org_hash(release.version, hash_cache),
        )

    stable = find_release(releases, "stable")
    updates[find_stable_entry_name(entries)] = KernelUpdate(
        name=branch_name(stable.version),
        tag=stable.version,
        version=stable.version,
        source="kernel-org",
        sha256=fetch_kernel_org_hash(stable.version, hash_cache),
    )

    if update_rc:
        mainline = find_release(releases, "mainline")
        if "-rc" not in mainline.version:
            raise RuntimeError(
                f"kernel.org mainline release {mainline.version} is not an rc"
            )
        if mainline.source is None:
            raise RuntimeError(f"kernel.org did not report a source URL for {mainline.version}")
        updates[find_rc_entry_name(entries)] = KernelUpdate(
            name=branch_name(mainline.version),
            tag=f"v{mainline.version}",
            version=rc_nix_version(mainline.version),
            source="torvalds",
            sha256=prefetch_rc_hash(mainline.source, mainline.version),
        )

    return updates


def find_stable_entry_name(entries: list[Entry]) -> str:
    candidates = [
        entry
        for entry in entries
        if re.fullmatch(r"\d+\.\d+", entry.name)
        and (version := field_value(entry.block, "version")) is not None
        and "-rc" not in version
    ]
    if len(candidates) != 1:
        names = ", ".join(entry.name for entry in candidates) or "<none>"
        raise RuntimeError(f"Expected one stable UKCI entry, found: {names}")
    return candidates[0].name


def find_rc_entry_name(entries: list[Entry]) -> str:
    candidates = [
        entry
        for entry in entries
        if re.fullmatch(r"\d+\.\d+", entry.name)
        and (version := field_value(entry.block, "version")) is not None
        and re.fullmatch(r"\d+\.\d+\.0-rc\d+", version)
    ]
    if len(candidates) != 1:
        names = ", ".join(entry.name for entry in candidates) or "<none>"
        raise RuntimeError(f"Expected one rc UKCI entry, found: {names}")
    return candidates[0].name


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--ukci-file",
        type=Path,
        default=UKCI_PATH,
        help=f"Path to UKCI Nix file (default: {UKCI_PATH})",
    )
    parser.add_argument(
        "--no-update-rc",
        action="store_true",
        help="Update stable/longterm entries only and leave the rc entry unchanged",
    )
    args = parser.parse_args()

    current_text = args.ukci_file.read_text()
    releases = load_releases()
    updated_text = apply_updates(
        current_text,
        build_updates(current_text, releases, update_rc=not args.no_update_rc),
    )

    if updated_text == current_text:
        print("UKCI kernels are already up to date")
        return 0

    args.ukci_file.write_text(updated_text)
    print(f"Updated {args.ukci_file}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
