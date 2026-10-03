---
description: "Functions: list, versions, logs, deploy"
argument-hint: "[name]"
---

Work with this airdress's functions. With no argument, call
`function_list` and report each function, the version it serves, and
whether it is ready.

With `$1`, report that one function: `function_versions` for its history
and `function_logs` for what it has been doing.

To change what it serves:

1. `function_validate` on the directory first, always. It asks the
   operator what a deploy would do and writes nothing.
2. Show the user what would change, and wait.
3. `function_deploy` when they agree.

Two refusals worth reading rather than retrying. `source_base_stale`
means somebody published since this tree was read — say who and when,
and read the current version before trying again. A deploy that is
refused for a signature is a signing-key problem, not something to
attempt a second time.
