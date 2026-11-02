#!/usr/bin/env python3
"""Check that the published tree contains only public material (Python 3.11+).

The publication set is what Git would track: tracked files plus untracked
files that are not ignored. Paths must stay under the allowed roots, private
names and suffixes are rejected, and public Markdown documents may only link
inside the publication set and may not mention internal material.
"""
import argparse
import re
import subprocess
import sys
from pathlib import Path, PurePosixPath
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
PUBLIC_ROOTS = {
    ".github",
    ".gitignore",
    "CHANGELOG.md",
    "COMMERCIAL-LICENSE.md",
    "Cargo.lock",
    "Cargo.toml",
    "LICENSE",
    "LICENSES",
    "README.md",
    "THIRD_PARTY_NOTICES.md",
    "crates",
    "docs",
    "fuzz",
    "rust-toolchain.toml",
    "tests",
    "tools",
}
PRIVATE_PARTS = {".internal", "reports", "data", "target", "artifacts", "__pycache__", ".env"}
PRIVATE_NAMES = re.compile(r"^(?:implementation-plan\.md|m\d+-[a-z0-9-]+\.md|development-full\.md)$")
PRIVATE_SUFFIXES = {".catpart", ".catproduct", ".catdrawing", ".catshape", ".cgr", ".stp", ".step", ".log", ".so", ".pyc", ".zip"}
# Internal directories, local machines, milestone shorthand and vendor or
# reference tooling that is not part of the public tree.
DOC_PRIVATE = re.compile(
    r"(?:\.internal/|\breports/|/home/|/Users/|/tmp/|\bM\d\b|\bHOOPS\b|hoops_|\bCAA\b|"
    r"chatgpt\.com|\bSpatial\b|\bDatakit\b|\bCrossManager\b|\bTransMagic\b|implementation-plan\.md|m\d-[a-z-]+\.md)"
)


def publication_set(root):
    output = subprocess.check_output(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"], cwd=root
    )
    return sorted(name for name in output.decode().split("\0") if name)


def check_paths(names):
    for name in names:
        p = PurePosixPath(name)
        if (
            p.is_absolute()
            or ".." in p.parts
            or p.parts[0] not in PUBLIC_ROOTS
            or PRIVATE_PARTS.intersection(p.parts)
            or PRIVATE_NAMES.match(p.name)
            or p.suffix.lower() in PRIVATE_SUFFIXES
        ):
            raise ValueError(f"nonpublic path: {name}")


def is_public_document(name):
    return name.endswith(".md") and (
        "/" not in name or name.startswith(("docs/", "fuzz/", "tests/", "tools/", "crates/"))
    )


def check_document(name, text, names):
    match = DOC_PRIVATE.search(text)
    if match:
        raise ValueError(f"internal reference in public documentation: {name}: {match.group(0)!r}")
    for target in re.findall(r"\[[^\]]*\]\(([^\s)]+)\)", text):
        url = urlsplit(target.strip("<>"))
        if url.scheme or url.netloc or not url.path:
            continue
        parts = list(PurePosixPath(name).parent.parts)
        for part in PurePosixPath(unquote(url.path)).parts:
            if part == "..":
                if not parts:
                    raise ValueError(f"link escapes source tree: {name}: {target}")
                parts.pop()
            elif part != ".":
                parts.append(part)
        resolved = "/".join(parts)
        if resolved not in names and not any(n.startswith(resolved + "/") for n in names):
            raise ValueError(f"link missing from public tree: {name}: {target}")


def check_tree(names, read):
    names = set(names)
    check_paths(names)
    for name in sorted(names):
        if is_public_document(name):
            check_document(name, read(name).decode("utf-8"), names)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args()
    names = publication_set(args.root)
    if not names:
        raise ValueError("no publishable files found")

    def read(name):
        path = args.root / name
        if path.is_symlink():
            raise ValueError(f"symlink in public tree: {name}")
        return path.read_bytes()

    check_tree(names, read)
    print(f"Public tree passed: {len(names)} files")


if __name__ == "__main__":
    try:
        main()
    except ValueError as error:
        print(f"public tree check failed: {error}", file=sys.stderr)
        sys.exit(1)
