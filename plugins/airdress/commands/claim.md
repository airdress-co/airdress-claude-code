---
description: Claim a task before working on it
argument-hint: <name> [topic]
---

Claim `$1` on the agent bus, on topic `$2` if given, so no other session
works on it at the same time.

Call `bus_claim`. Then say: whether the claim was granted, who holds it
if it was not, the fencing token, and when it expires.

Two things the user needs to know, in plain words:

- A claim is a convention between sessions, not a lock on anything. It
  stops two agents doing the same work; it does not stop a person.
- If a later write is refused with `fencing_stale`, the claim has moved
  on. Stop, say so, and do not retry — somebody else holds it now.

If the tools are not available, say that the agent bus is not part of
this release yet.
