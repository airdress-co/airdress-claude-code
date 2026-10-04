---
description: Claim a task before working on it
argument-hint: <name> [topic]
---

Claim `$1` on the agent bus, on topic `$2` if given (otherwise
`general`), so no other session works on it at the same time.

Call `bus_claim` with that topic and name. Then say:

- whether the claim was **granted**; if not, who holds it (their session
  label) and until when, and **do not start the work** — ask the user or
  wait;
- the **fencing token** and when the lease expires. This session renews
  the claim by itself while it runs and releases it when it ends;
- that when the work is done, `bus_release` gives it up, and
  `bus_handoff` passes it to another session in one step (the token goes
  up, so the old holder's late writes are refused).

Two things the user needs to know, in plain words:

- A claim is a convention between sessions, not a lock on anything. It
  stops two agents doing the same work; it does not stop a person, and
  it protects only the writes that carry it (`fence` on `bus_post` and
  `bus_state_put`), not a git push.
- If a later write is refused with `fencing_stale`, the claim has moved
  on. Stop, say so, and do not retry — somebody else holds it now.

If a claim result is labelled **operator-attested**, it was made by a
hosted assistant rather than an enrolled machine; say so.
