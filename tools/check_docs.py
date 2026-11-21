#!/usr/bin/env python3
"""Check RustCraft's documentation paths and local Markdown links."""

from functools import cache
from html import unescape
from pathlib import Path
import re
import sys
from urllib.parse import unquote, urlsplit


ROOT = Path(__file__).resolve().parents[1]
ENTRY_POINTS = ("README.md", "AGENTS.md")
KEBAB_CASE = re.compile(r"[a-z0-9]+(?:-[a-z0-9]+)*")
FENCE = re.compile(r"^ {0,3}(`{3,}|~{3,})(.*)$")
CODE_SPAN = re.compile(r"(`+)(?!`).*?(?<!`)\1(?!`)")
HEADING = re.compile(r"^ {0,3}#{1,6}\s+(.*)$")
EMPHASIS = re.compile(r"(?<!\w)(_{1,3})(?=\S)(.+?)(?<=\S)\1(?!\w)")
ANCHOR = re.compile(r"<a\b[^>]*\b(?:id|name)\s*=\s*[\"']([^\"']+)[\"']")
DESTINATION = r"(?:<([^>\n]*)>|((?:[^\s()<>]+|\([^()\s<>]*\))+))"
LINK = re.compile(r"(?<!\\)\[[^\]\n]*\]\(\s*" + DESTINATION + r"(?:\s|\))")
REFERENCE = re.compile(r"^ {0,3}\[[^\]\n]+\]:\s*" + DESTINATION)


@cache
def markdown_lines(path):
    lines = []
    opener = None
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        fence = FENCE.match(line)
        if opener is not None:
            if fence:
                marker, rest = fence.groups()
                if marker[0] == opener[0] and len(marker) >= len(opener) and not rest.strip():
                    opener = None
        elif fence:
            opener = fence[1]
        else:
            lines.append((number, line))
    return lines


@cache
def markdown_anchors(path):
    explicit = set()
    headings = set()
    for _, line in markdown_lines(path):
        explicit.update(ANCHOR.findall(CODE_SPAN.sub("", line)))
        title = HEADING.match(line)
        if title is None:
            continue
        text = re.sub(r"\s+#+\s*$", "", unescape(title[1]).strip().lower())
        text = EMPHASIS.sub(r"\2", text)
        base = re.sub(r"[^\w -]", "", text).replace(" ", "-")
        anchor = base
        suffix = 0
        while anchor in headings:
            suffix += 1
            anchor = f"{base}-{suffix}"
        headings.add(anchor)
    return explicit | headings


def local_link_error(document, destination):
    url = urlsplit(destination)
    if url.scheme == "file" or (not url.netloc and url.path.startswith("/")):
        return "use a repository-relative link"
    if url.scheme or url.netloc:
        return None

    target = (document.parent / unquote(url.path)).resolve() if url.path else document
    if not target.is_relative_to(ROOT):
        return "link leaves the repository"
    if not target.exists():
        return "target does not exist"
    if not url.fragment:
        return None

    if target.is_dir():
        target /= "README.md"
    if target.suffix == ".md":
        if not target.is_file():
            return "directory does not have a README for this fragment"
        if unquote(url.fragment) not in markdown_anchors(target):
            return "Markdown anchor does not exist"
    return None


def markdown_files(directory):
    return sorted(path for path in directory.rglob("*") if path.is_file() and path.suffix.lower() == ".md")


def main():
    documents = markdown_files(ROOT / "doc")
    errors = []
    for name in ENTRY_POINTS + ("doc/README.md",):
        if not (ROOT / name).is_file():
            errors.append(f"{name} | missing documentation entry point")
    for path in ROOT.iterdir():
        if path.is_file() and path.suffix.lower() == ".md" and path.name not in ENTRY_POINTS:
            errors.append(f"{path.name} | move engineering Markdown into doc/")
    for path in markdown_files(ROOT / "docs"):
        errors.append(f"{path.relative_to(ROOT)} | move engineering Markdown into doc/")
    for path in documents:
        relative = path.relative_to(ROOT / "doc")
        if any(not KEBAB_CASE.fullmatch(part) for part in relative.parts[:-1]) or (
            path.name != "README.md"
            and (path.suffix != ".md" or not KEBAB_CASE.fullmatch(path.stem))
        ):
            errors.append(f"doc/{relative} | use lowercase words separated by hyphens")

    documents.extend(ROOT / name for name in ENTRY_POINTS if (ROOT / name).is_file())
    for document in documents:
        relative = document.relative_to(ROOT)
        try:
            lines = markdown_lines(document)
        except (OSError, UnicodeError) as error:
            errors.append(f"{relative} | {error}")
            continue
        for number, line in lines:
            line = CODE_SPAN.sub("", line)
            reference = REFERENCE.match(line)
            links = [reference] if reference else LINK.finditer(line)
            for link in links:
                target = link[1] if link[1] is not None else link[2]
                try:
                    error = local_link_error(document, target)
                except (OSError, UnicodeError, ValueError) as failure:
                    error = str(failure)
                if error:
                    errors.append(f"{relative}:{number} | {target!r} | {error}")

    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(f"Checked {len(documents)} Markdown files and their local links.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
