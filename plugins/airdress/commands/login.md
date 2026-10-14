---
description: Sign in to Airdress
argument-hint: "[device]"
---

Sign the user in.

Call the `login` tool. If `$1` is `device`, or the user says they will
sign in on another device, pass `device: true`.

- **`signed_in: true`**: the browser opened and they approved. Say in
  one line who is signed in, then any `notices` in their own words.
- **`signed_in: false` with `opened_browser: true`**: a browser was
  opened here and is still waiting. Show the URL on its own line, in
  case it opened out of sight, and ask them to say when they are done.
- **`signed_in: false` with a `user_code`**: no browser here. Show the
  URL and the code on their own lines, exactly as given; they work on
  any device. Ask them to say when they are done.

In both waiting cases, say any `notices` too, and when the user says
they are done, call `whoami`. If it still reports nobody signed in, say
so and offer to start again; do not poll in a loop.
