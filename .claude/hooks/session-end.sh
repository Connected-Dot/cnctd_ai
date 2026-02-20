#!/bin/bash
# Session End Hook - Log session end and reminder

SUMMARIES_DIR="$HOME/.claude/session-summaries"
PROJECT_NAME="cnctd_ai"
TIMESTAMP=$(date +"%Y-%m-%d_%H-%M-%S")

mkdir -p "$SUMMARIES_DIR"

INPUT=$(cat)
REASON=$(echo "$INPUT" | grep -o '"reason":"[^"]*"' | cut -d'"' -f4)

echo "$(date '+%Y-%m-%d %H:%M:%S') - Session ended ($REASON) in $PWD" >> "$SUMMARIES_DIR/session-log.txt"

echo ""
echo "=========================================="
echo "Session ended ($PROJECT_NAME)"
echo "  Time: $(date '+%Y-%m-%d %H:%M:%S')"
echo ""
echo "If you didn't run /session-summary, consider it"
echo "for the next session's context."
echo "=========================================="

exit 0
