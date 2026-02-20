#!/bin/bash
# Session Start Hook - Review previous session summary
# Reads from project's docs/SESSION_*.md files

# Find the most recent SESSION_*.md file
LATEST_SESSION=$(ls -t docs/SESSION_*.md 2>/dev/null | head -1)

if [ -z "$LATEST_SESSION" ]; then
    exit 0
fi

SUMMARY_DATE=$(stat -f "%Sm" -t "%Y-%m-%d %H:%M" "$LATEST_SESSION" 2>/dev/null || stat -c "%y" "$LATEST_SESSION" 2>/dev/null | cut -d'.' -f1)
SUMMARY_CONTENT=$(cat "$LATEST_SESSION")
FILENAME=$(basename "$LATEST_SESSION")

ESCAPED_CONTENT=$(echo "$SUMMARY_CONTENT" | sed 's/\\/\\\\/g' | sed 's/"/\\"/g' | tr '\n' ' ' | sed 's/  */ /g')

cat << EOF
{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "Previous session summary ($FILENAME from $SUMMARY_DATE): $ESCAPED_CONTENT"
  }
}
EOF

exit 0
