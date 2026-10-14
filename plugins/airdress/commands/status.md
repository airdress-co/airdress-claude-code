---
description: Account, airdress, capabilities, agent device, bus, verification
---

Report where this session stands. Call `whoami`. Be brief: when
everything is as expected, the whole answer is two or three lines.

**First, and only if present**, before anything else:

- `launcher.override_in_force`: the server is running with an override,
  so this binary may not be the verified release. Say so and give the
  reason it carries.
- any `notices`, each in its own words. The one that exists today: the
  profile still holds the old sign-in, whose one token every airdress
  accepts; the fix is `/airdress:login`.

**Then the summary**, in two or three lines:

- who is signed in and to which airdress (its FQDN). If nobody is
  signed in, say to run `/airdress:login` and stop;
- that the server was verified, and from which origin (`cdn` or
  `github`).

**Add a line only for what is not as expected**, one line each:

- the sign-in is not the hub's (`sign_in_kind` other than `hub`), or the
  token is expired and not refreshable;
- a capability that is off: say it is off on this airdress and give the
  link `whoami` carries. Do not guess at why, and do not mention plans
  or prices;
- no agent device on this machine: the bus can be read but not written;
- bus `notices` or claims this session holds.

Do not explain the verification steps, the delivery model, or anything
else that is working, unless the user asks. They can ask for detail.
