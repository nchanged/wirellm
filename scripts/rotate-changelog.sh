#!/usr/bin/env bash
# Rotate CHANGELOG.md [Unreleased] section into [X.Y.Z] - YYYY-MM-DD.
#
# Usage:
#   scripts/rotate-changelog.sh <new_version> [date]
#
# Behaviour:
#   1. Verifies CHANGELOG.md has a [Unreleased] section with at least one
#      bullet (a line starting with '- '). Fails with exit 1 if empty —
#      this is the gate that stops a release from shipping with no notes.
#   2. Renames `## [Unreleased]` → `## [<new_version>] - <date>`.
#   3. Inserts a fresh empty `## [Unreleased]` block above it.
#
# Date defaults to UTC YYYY-MM-DD if not provided.
#
# The check is intentionally simple: presence of any bullet under
# [Unreleased] before the next ## header. If you want to override (rare:
# pure plumbing release), add a one-line "### Internal" note explaining
# why — that satisfies the bullet requirement.

set -euo pipefail

if [[ $# -lt 1 ]]; then
    echo "usage: $0 <new_version> [date]" >&2
    exit 2
fi

NEW_VERSION="$1"
RELEASE_DATE="${2:-$(date -u +%Y-%m-%d)}"
CHANGELOG="${CHANGELOG_PATH:-CHANGELOG.md}"

if [[ ! -f "$CHANGELOG" ]]; then
    echo "error: $CHANGELOG not found" >&2
    exit 1
fi

# Extract content between '## [Unreleased]' and the next '## ' header.
unreleased_body=$(awk '
    /^## \[Unreleased\]/ { in_section = 1; next }
    in_section && /^## / { in_section = 0 }
    in_section { print }
' "$CHANGELOG")

# Require at least one bullet line.
if ! grep -qE '^- ' <<<"$unreleased_body"; then
    cat >&2 <<EOF
error: CHANGELOG.md [Unreleased] section is empty.

Releases must ship with user-facing notes. Add at least one bullet
under '## [Unreleased]' describing what users can now do (or what
bug they no longer hit). See AGENTS.md → Changelog discipline.

If this is a pure plumbing release with no user impact, add:

    ### Internal
    - <one-line reason>

and re-run.
EOF
    exit 1
fi

# Rotate: rename [Unreleased] → [X.Y.Z] - DATE, prepend fresh [Unreleased].
# Use a temp file to avoid in-place sed portability issues (BSD vs GNU).
tmp=$(mktemp)
awk -v ver="$NEW_VERSION" -v date="$RELEASE_DATE" '
    /^## \[Unreleased\]/ && !done {
        print "## [Unreleased]"
        print ""
        print "## [" ver "] - " date
        done = 1
        next
    }
    { print }
' "$CHANGELOG" > "$tmp"
mv "$tmp" "$CHANGELOG"

echo "rotated CHANGELOG.md: [Unreleased] → [$NEW_VERSION] - $RELEASE_DATE"
