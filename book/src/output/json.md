# JSON schema

`agentop --json` prints a single-line snapshot to stdout and exits.
`agentop --json --pretty` indents it for human reading.

The schema is versioned. `schema_version` is currently **`1`** and will not
remove or rename fields during the 1.x release line. Additive changes
(new fields) are allowed without a version bump. Breaking changes
increment the number.

The authoritative, machine-checked reference lives at
[`docs/output-schema.md`](https://github.com/leboiko/claude-codex-pid-inspector/blob/master/docs/output-schema.md)
in the repository and is included below verbatim.

{{#include ../../../docs/output-schema.md}}
