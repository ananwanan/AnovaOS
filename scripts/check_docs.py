#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# Validate bilingual pages, language navigation, purpose comments and local Markdown links.
"""Check bilingual document coverage, language navigation, purposes and local Markdown links."""
from pathlib import Path
import re
import subprocess
import sys
import unicodedata
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "docs"
LANGUAGES = ("en", "zh-CN")


def prose(text, keep_inline_code=False):
    """Exclude fenced examples and inline code from Markdown navigation checks."""
    result = []
    fence = None
    for line in text.splitlines():
        marker = re.match(r"^\s{0,3}(`{3,}|~{3,})", line)
        if marker:
            value = marker.group(1)
            if fence is None:
                fence = value
            elif value[0] == fence[0] and len(value) >= len(fence):
                fence = None
            continue
        if fence is None:
            result.append(line if keep_inline_code else re.sub(r"(`+).*?\1", "", line))
    return "\n".join(result)


def anchors(text):
    """Recognize explicit IDs and GitHub-style heading fragments, including duplicate suffixes."""
    visible = prose(text, keep_inline_code=True)
    found = set(re.findall(r'\bid=[\'\"]([^\'\"]+)[\'\"]', visible))
    counts = {}
    for line in visible.splitlines():
        match = re.match(r"^#{1,6}\s+(.+?)\s*#*\s*$", line)
        if not match:
            continue
        heading = re.sub(r"\[([^]]+)\]\([^)]*\)", r"\1", match.group(1))
        heading = re.sub(r"(`+)(.*?)\1", r"\2", heading)
        heading = re.sub(r"<[^>]*>", "", heading).lower()
        slug = "".join(
            char
            for char in heading
            if char in "-_ " or not unicodedata.category(char).startswith(("P", "S"))
        )
        slug = slug.replace(" ", "-")
        number = counts.get(slug, 0)
        counts[slug] = number + 1
        found.add(f"{slug}-{number}" if number else slug)
    return found


def main():
    errors = []
    trees = {
        lang: {path.relative_to(DOCS / lang) for path in (DOCS / lang).rglob("*.md")}
        for lang in LANGUAGES
    }
    for lang, other in (("en", "zh-CN"), ("zh-CN", "en")):
        for missing in sorted(trees[lang] - trees[other]):
            errors.append(f"docs/{other}/{missing}: missing translation")
    output = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=ROOT
    )
    files = sorted(
        {
            ROOT / name
            for name in output.decode().split("\0")
            if name.endswith(".md") and (ROOT / name).is_file()
        }
    )
    texts = {p: p.read_text(encoding="utf-8") for p in files}
    ids = {path: anchors(value) for path, value in texts.items()}
    for path, text in texts.items():
        relative = path.relative_to(ROOT)
        if not re.search(r"<!--\s*\S.+?-->", "\n".join(text.splitlines()[:8]), re.S):
            errors.append(f"{relative}: missing English file-purpose comment")
        counterpart = None
        if path.is_relative_to(DOCS / "en") or path.is_relative_to(DOCS / "zh-CN"):
            language = relative.parts[1]
            other = "zh-CN" if language == "en" else "en"
            counterpart = DOCS / other / path.relative_to(DOCS / language)
        elif path.name in {"README.md", "README.zh-CN.md"}:
            other = "README.zh-CN.md" if path.name == "README.md" else "README.md"
            counterpart = path.with_name(other)
            if not counterpart.is_file():
                errors.append(f"{relative}: missing translation")
        if counterpart is not None:
            destinations = re.findall(
                r"\[[^]\n]*\]\(([^)\s]+)\)", "\n".join(text.splitlines()[:12])
            )
            if not any(
                (path.parent / unquote(urlsplit(link).path)).resolve() == counterpart
                for link in destinations
                if not urlsplit(link).scheme
            ):
                errors.append(f"{relative}: missing reciprocal language link near top")
        for match in re.finditer(r"!?\[[^]\n]*\]\((<?[^)\n]+>?)\)", prose(text)):
            destination = match.group(1).strip().strip("<>")
            if re.match(r"[a-zA-Z][a-zA-Z0-9+.-]*:", destination):
                continue
            link = urlsplit(destination)
            if not link.path:
                target = path
            elif link.path.startswith("/"):
                target = (ROOT / unquote(link.path).lstrip("/")).resolve()
            else:
                target = (path.parent / unquote(link.path)).resolve()
            if not target.is_relative_to(ROOT):
                errors.append(f"{relative}: link escapes repository: {destination}")
            elif not target.exists():
                errors.append(f"{relative}: missing local target: {destination}")
            elif (
                link.fragment
                and target.suffix == ".md"
                and unquote(link.fragment) not in ids.get(target, set())
            ):
                errors.append(f"{relative}: missing heading: {destination}")
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(
        f"Documentation checked: {len(trees['en'])} paired pages, {len(files)} Markdown files; "
        "local links and language navigation passed"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
