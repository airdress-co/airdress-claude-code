---
description: Account, airdress, capabilities, agent device, bus, verification
---

Report where this session stands. Call `whoami` and write out, in this
order and in prose rather than a table:

1. **Notices** — if `whoami` carries any `notices`, say each one here,
   in its own words (only a launcher override, below, comes before
   them). Today the only one is that the profile still holds the old
   sign-in, whose one token every airdress accepts; the fix is
   `/airdress:login`.
2. **Account** — the profile, the hub, who is signed in, the kind of
   sign-in (`sign_in_kind`: `hub` gives each airdress a token only it
   accepts; anything else is the old one-token sign-in), and whether
   the token still works. If nobody is signed in, stop here and say to run
   `/airdress:login`.
3. **Airdress** — which one this session acts on, and why it is that one
   (a tool argument, an environment variable, a `.airdress` marker, or
   the profile's default).
4. **What the airdress has turned on** — the four capabilities. For
   anything off, say that it is off on this airdress and give the link
   `whoami` carries. Do not guess at why, and do not mention plans or
   prices: the answer to "why" is on that page.
5. **Agent device and bus** — whether this session is on the bus (its
   id, label, topics and the claims it holds), any bus `notices`, and how
   messages are delivered (`delivery`): pushed as channel events when the
   Airdress channel is on, always readable with `bus_read`. Claude Code
   drops channel events silently when the channel is not loaded, so say
   that reading is the delivery that cannot be missed.
6. **How this server got here** — the launcher's verification and which
   origin served the bundle. **If an override is in force, say so
   first**, before anything else: an unverified binary is the most
   important thing on this list.
