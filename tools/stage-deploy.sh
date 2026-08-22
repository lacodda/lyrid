#!/bin/sh
# Assembles the directory that gets uploaded to a stand.
#
# turnout uploads a `dist` directory as it finds it, so `dist` has to contain
# exactly what the remote build needs - not the repository root, which also
# holds `target/` (gigabytes of host-architecture artifacts) and, on a machine
# that has run the stand locally, a `.env`.
#
# `git archive` is the filter: it emits tracked files and nothing else, so a
# secret that was never committed cannot be uploaded by accident.
set -eu

out=${1:-deploy}

if ! git rev-parse --git-dir >/dev/null 2>&1; then
    echo "stage-deploy: not inside a git repository" >&2
    exit 1
fi

rm -rf "$out"
mkdir -p "$out"

# HEAD, not the working tree: what reaches the stand is a committed state, so
# "which version is running" always has an answer.
git archive --format=tar HEAD | tar -x -C "$out"

echo "staged $(find "$out" -type f | wc -l | tr -d ' ') files into $out/"
