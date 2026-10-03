---
description: Sign in to Airdress
---

Sign the user in, then say who they are.

1. Call the `login` tool. It answers with a URL and a short code, and
   nothing else: the sign-in finishes in the user's browser, not here.
2. Show the user the URL and the code, plainly, on their own lines. Do
   not shorten the URL or re-type the code.
3. Wait for the user to say they have finished, then call `whoami`.

If `whoami` still reports nobody signed in, say so and offer to start
again — do not poll in a loop.
