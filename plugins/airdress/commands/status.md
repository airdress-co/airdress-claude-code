---
description: Account, airdress, capabilities, agent device, bus, verification
---

Report where this session stands. Call `whoami` and write out, in this
order and in prose rather than a table:

1. **Account** — the profile, the hub, who is signed in, and whether the
   token still works. If nobody is signed in, stop here and say to run
   `/airdress:login`.
2. **Airdress** — which one this session acts on, and why it is that one
   (a tool argument, an environment variable, a `.airdress` marker, or
   the profile's default).
3. **What the airdress has turned on** — the four capabilities. For
   anything off, say that it is off on this airdress and give the link
   `whoami` carries. Do not guess at why, and do not mention plans or
   prices: the answer to "why" is on that page.
4. **Agent device and bus** — their state, or that this release does not
   include them.
5. **How this server got here** — the launcher's verification and which
   origin served the bundle. **If an override is in force, say so
   first**, before anything else: an unverified binary is the most
   important thing on this list.
