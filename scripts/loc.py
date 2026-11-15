#!/usr/bin/env python3
"""Lines of code per language over the history of main, as a plot.

    loc.py HISTORY.csv PLOT.svg

HISTORY.csv has a row per commit and language (commit, date, language,
code). The commits of main (its first parents) that aren't in it are
counted with tokei, each from `git archive` of it, and added; then the plot
is drawn from all of them, a line per language. Run in the repo, with its
whole history (a clone with fetch-depth 0).
"""

import csv
import json
import os
import subprocess
import sys
import tempfile

# the languages with fewer lines than this, at the last commit, are left out
SMALLEST = 200


def git(*args):
    return subprocess.run(["git", *args], check=True, capture_output=True, text=True).stdout


def count(commit):
    """Lines of code per language at COMMIT, by tokei."""
    with tempfile.TemporaryDirectory() as d:
        archive = subprocess.run(["git", "archive", commit], check=True, capture_output=True).stdout
        subprocess.run(["tar", "-x", "-C", d], input=archive, check=True)
        out = subprocess.run(["tokei", "--output", "json", d], check=True, capture_output=True, text=True).stdout
    langs = json.loads(out)
    return {name: v["code"] for name, v in langs.items() if name != "Total" and v.get("code", 0) > 0}


def main(history, plot):
    rows = []
    if os.path.exists(history):
        with open(history, newline="") as f:
            rows = list(csv.DictReader(f))
    seen = {r["commit"] for r in rows}
    commits = git("log", "--first-parent", "--reverse", "--format=%H %cI", "HEAD").split("\n")
    for line in filter(None, commits):
        commit, date = line.split(" ", 1)
        if commit in seen:
            continue
        for lang, code in sorted(count(commit).items()):
            rows.append({"commit": commit, "date": date, "language": lang, "code": str(code)})
    with open(history, "w", newline="") as f:
        w = csv.DictWriter(f, ["commit", "date", "language", "code"])
        w.writeheader()
        w.writerows(rows)
    draw(rows, plot)


def draw(rows, plot):
    import matplotlib

    matplotlib.use("svg")
    import matplotlib.dates as mdates
    import matplotlib.pyplot as plt
    from datetime import datetime

    dates = sorted({r["date"] for r in rows})
    by = {}
    for r in rows:
        by.setdefault(r["language"], {})[r["date"]] = int(r["code"])
    last = dates[-1]
    langs = sorted((l for l in by if by[l].get(last, 0) >= SMALLEST), key=lambda l: -by[l][last])
    xs = [datetime.fromisoformat(d) for d in dates]
    fig, ax = plt.subplots(figsize=(9, 4.5), dpi=100)
    for lang in langs:
        ax.plot(xs, [by[lang].get(d, 0) for d in dates], label=f"{lang} ({by[lang][last]:,})", linewidth=1.8)
    ax.set_ylabel("lines of code")
    ax.set_title(f"lineit: lines of code per language ({sum(by[l][last] for l in langs):,} in all)")
    ax.xaxis.set_major_formatter(mdates.DateFormatter("%Y-%m-%d"))
    ax.grid(True, alpha=0.3)
    ax.legend(loc="upper left", fontsize="small")
    fig.autofmt_xdate()
    fig.tight_layout()
    fig.savefig(plot)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2])
