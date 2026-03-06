#!/bin/bash
#
# Terrarium PreToolUse hook for Bash commands.
# Proxies shell commands into the project's dev container via
# limactl shell → nerdctl exec, so Claude Code runs commands
# in the sandboxed environment transparently.
#
# Input (stdin): JSON with tool_input.command, cwd
# Output (stdout): JSON with permissionDecision + rewritten command

set -euo pipefail

INPUT=$(cat)
COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command')
CWD=$(echo "$INPUT" | jq -r '.cwd')

# Find project root (directory containing .terrarium/config.json)
PROJECT_ROOT="$CWD"
while [ "$PROJECT_ROOT" != "/" ]; do
  [ -f "$PROJECT_ROOT/.terrarium/config.json" ] && break
  PROJECT_ROOT=$(dirname "$PROJECT_ROOT")
done

if [ ! -f "$PROJECT_ROOT/.terrarium/config.json" ]; then
  # Not a Terrarium project — pass through without rewriting
  exit 0
fi

CONTAINER=$(jq -r '.container_name' "$PROJECT_ROOT/.terrarium/config.json")
WORKSPACE_ROOT="/home/terrarium/workspace"

# Map host cwd to container cwd
if [[ "$CWD" == "$PROJECT_ROOT"* ]]; then
  REL_PATH="${CWD#$PROJECT_ROOT}"
  CONTAINER_CWD="$WORKSPACE_ROOT$REL_PATH"
else
  CONTAINER_CWD="$WORKSPACE_ROOT"
fi

# Write command to a temp file to avoid shell quoting issues.
# limactl shell uses SSH which re-tokenizes arguments, so passing
# complex commands as args is fragile. The temp file + stdin pipe
# approach keeps the command text intact.
TMPFILE=$(mktemp /tmp/terrarium-cmd-XXXXXX)
echo "$COMMAND" > "$TMPFILE"

# Rewrite: pipe command through lima into container
REWRITTEN="cat '$TMPFILE' | limactl shell terrarium -- sudo nerdctl exec -i --user terrarium -w '$CONTAINER_CWD' '$CONTAINER' bash -l; EXIT_CODE=\$?; rm -f '$TMPFILE'; exit \$EXIT_CODE"

jq -n --arg cmd "$REWRITTEN" '{
  hookSpecificOutput: {
    hookEventName: "PreToolUse",
    permissionDecision: "allow",
    updatedInput: { command: $cmd }
  }
}'
