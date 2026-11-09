#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Measure the codebase: lines per language and crate, files over the size
budget, long functions, and how the editor's UI scripts share names.

    python3 tools/code-metrics.py [--all]

Production lines leave out a Rust file's `#[cfg(test)]` part and test files.
The budget and the reasoning are in docs/design/structure.md; the test
`crates/amb-cli/tests/code_size.rs` holds it. --all lists every file, not
only the big ones.
"""
import collections
import os
import re
import subprocess
import sys

RUST_BUDGET = 800
JS_BUDGET = 700
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Comments, strings and chars, matched left to right so a quote in a comment
# or a // in a string can't fool the other; each is replaced by its newlines.
RUST_NOISE = re.compile(
    r"//[^\n]*|/\*.*?\*/|(?<![A-Za-z0-9_])b?r(#*)\".*?\"\1|b?\"(?:\\.|[^\"\\])*\"|b?'(?:\\.|[^'\\])'",
    re.S,
)
JS_NOISE = re.compile(r"//[^\n]*|/\*.*?\*/|`(?:\\.|[^`\\])*`|'(?:\\.|[^'\\\n])*'|\"(?:\\.|[^\"\\\n])*\"", re.S)


def quiet(source, noise):
    return noise.sub(lambda m: "\n" * m.group(0).count("\n"), source)


def git_files(pattern):
    out = subprocess.run(["git", "ls-files", pattern], cwd=ROOT, capture_output=True, text=True)
    return [f for f in out.stdout.split("\n") if f]


def read(path):
    with open(os.path.join(ROOT, path), encoding="utf-8", errors="replace") as f:
        return f.read()


def is_rust_test_file(path):
    return "/tests/" in path or path.endswith("/tests.rs")


def rust_production_lines(source, path):
    """Lines before the first `#[cfg(test)]` at the start of a line."""
    if is_rust_test_file(path):
        return 0
    lines = source.split("\n")
    at = next((i for i, l in enumerate(lines) if l.startswith("#[cfg(test)]")), None)
    return len(lines) if at is None else at


def functions(lines, header):
    """(length, line, name) of each function, by matching braces."""
    found = []
    for i, line in enumerate(lines):
        m = header.match(line)
        if not m or line.rstrip().endswith(";"):
            continue
        depth, started = 0, False
        for j in range(i, len(lines)):
            depth += lines[j].count("{") - lines[j].count("}")
            started = started or "{" in lines[j]
            if started and depth <= 0:
                break
        found.append((j - i + 1, i + 1, m.group("name")))
    return found


RUST_FN = re.compile(r"\s*(pub(\([^)]*\))?\s+)?(const\s+)?(async\s+)?(unsafe\s+)?fn\s+(?P<name>\w+)")
JS_FN = re.compile(r"\s*(async\s+)?function\s*\*?\s*(?P<name>\w+)")
JS_TOP = re.compile(r"^(?:async\s+)?function\s+(\w+)|^(?:const|let|var|class)\s+(\w+)", re.M)


def main():
    show_all = "--all" in sys.argv

    print("== Lines per language")
    for ext, label in [("rs", "Rust"), ("js", "JavaScript"), ("wgsl", "WGSL"), ("html", "HTML"), ("css", "CSS"), ("md", "Markdown")]:
        files = git_files(f"*.{ext}")
        print(f"{label:11s} {len(files):4d} files {sum(read(f).count(chr(10)) for f in files):8,d} lines")

    rust = git_files("*.rs")
    sources = {f: read(f) for f in rust}
    print("\n== Rust lines per crate (tests included)")
    crates = collections.Counter()
    for f, s in sources.items():
        parts = f.split("/")
        crates[parts[1] if parts[0] == "crates" else parts[0]] += s.count("\n")
    for name, n in crates.most_common():
        print(f"{n:7,d}  {name}")

    print(f"\n== Rust files over {RUST_BUDGET} production lines" if not show_all else "\n== Rust files by production lines")
    prod = sorted(((rust_production_lines(s, f), f) for f, s in sources.items()), reverse=True)
    for n, f in prod:
        if show_all or n > RUST_BUDGET:
            print(f"{n:5d}  {f}")

    ui_dir = "crates/amb-gui/ui"
    js = [f for f in git_files(f"{ui_dir}/*.js")]
    js_src = {f: read(f) for f in js}
    print(f"\n== UI scripts over {JS_BUDGET} lines" if not show_all else "\n== UI scripts by lines")
    for n, f in sorted(((s.count("\n"), f) for f, s in js_src.items()), reverse=True):
        if show_all or n > JS_BUDGET:
            print(f"{n:5d}  {f}")

    print("\n== Longest Rust functions outside tests")
    fns = []
    for f, s in sources.items():
        if is_rust_test_file(f):
            continue
        cut = rust_production_lines(s, f)
        lines = quiet(s, RUST_NOISE).split("\n")[:cut]
        fns += [(n, f, line, name) for n, line, name in functions(lines, RUST_FN)]
    for n, f, line, name in sorted(fns, reverse=True)[:25]:
        print(f"{n:4d}  {f}:{line}  {name}")
    lengths = [x[0] for x in fns]
    print("over " + ", ".join(f"{t}: {sum(1 for l in lengths if l > t)}" for t in (50, 100, 150, 300)) + f" (of {len(lengths)})")

    print("\n== Longest UI functions")
    jfns = []
    for f, s in js_src.items():
        lines = quiet(s, JS_NOISE).split("\n")
        jfns += [(n, f, line, name) for n, line, name in functions(lines, JS_FN)]
    for n, f, line, name in sorted(jfns, reverse=True)[:15]:
        print(f"{n:4d}  {f}:{line}  {name}")
    print(f"over 80 lines: {sum(1 for x in jfns if x[0] > 80)} (of {len(jfns)})")

    # The UI's scripts share one global scope: which file's names the others use.
    own = {}
    for f, s in js_src.items():
        own[f] = {a or b for a, b in JS_TOP.findall(s)}
    users = collections.Counter()
    for f, s in js_src.items():
        words = set(re.findall(r"\b\w+\b", quiet(s, JS_NOISE)))
        for g in js_src:
            if g != f and own[g] & words:
                users[g] += 1
    print(f"\n== UI: {sum(len(v) for v in own.values())} top-level names; files whose names the most other files use")
    for g, n in users.most_common(10):
        print(f"{n:3d}  {g}")


if __name__ == "__main__":
    main()
