# Session Summary: a92bd1eb-6aeb-467b-9a93-2b234ec4cfac

**Total Hook Invocations:** 346

**Session Duration:** 5h5m21.298s

## Event Breakdown

- **CwdChanged**: 1
- **InstructionsLoaded**: 122
- **PermissionRequest**: 8
- **PostToolUse**: 42
- **PostToolUseFailure**: 3
- **PreToolUse**: 124
- **SessionStart**: 4
- **Stop**: 3
- **SubagentStart**: 2
- **SubagentStop**: 29
- **TaskCompleted**: 6
- **UserPromptSubmit**: 2

## Decision Breakdown

- **allow**: 121
- **ask**: 3

## Top 5 Slowest Hook Executions

| # | Event | Handler | Tool | Duration (ms) |
|---|-------|---------|------|---------------|
| 1 | SessionStart | *hook.sessionStartHandler |  | 88 |
| 2 | SessionStart | *hook.autoUpdateHandler |  | 49 |
| 3 | PreToolUse | *hook.preToolHandler | Edit | 39 |
| 4 | PreToolUse | *hook.preToolHandler | Edit | 39 |
| 5 | PreToolUse | *hook.preToolHandler | Edit | 39 |

## Errors (0)

_No errors recorded._

