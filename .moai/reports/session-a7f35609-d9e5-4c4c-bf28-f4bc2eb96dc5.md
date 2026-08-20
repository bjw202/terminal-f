# Session Summary: a7f35609-d9e5-4c4c-bf28-f4bc2eb96dc5

**Total Hook Invocations:** 309

**Session Duration:** 1h27m26.353s

## Event Breakdown

- **ConfigChange**: 4
- **InstructionsLoaded**: 70
- **PermissionRequest**: 2
- **PostToolUse**: 51
- **PreToolUse**: 91
- **SessionStart**: 4
- **Stop**: 8
- **SubagentStart**: 5
- **SubagentStop**: 66
- **UserPromptSubmit**: 8

## Decision Breakdown

- **allow**: 91

## Top 5 Slowest Hook Executions

| # | Event | Handler | Tool | Duration (ms) |
|---|-------|---------|------|---------------|
| 1 | SessionStart | *hook.autoUpdateHandler |  | 199 |
| 2 | SessionStart | *hook.sessionStartHandler |  | 61 |
| 3 | PostToolUse | *hook.postToolHandler | Edit | 8 |
| 4 | PostToolUse | *hook.postToolHandler | Write | 6 |
| 5 | PostToolUse | *hook.postToolHandler | Write | 6 |

## Errors (0)

_No errors recorded._

