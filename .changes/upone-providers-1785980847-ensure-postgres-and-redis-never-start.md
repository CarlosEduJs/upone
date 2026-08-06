---
crate: upone-providers
bump: patch
---

Ensure postgres and redis never start a second, redundant `docker compose` invocation: when a compose file defines them they depend on the docker provider's compose-up task and only verify the service responds, removing a race between concurrent compose runs.
Report a clear, actionable error when postgres or redis are detected without a compose service to start them, instead of a broken `docker compose up`.
Show the tail of a failing command's output as the task error message, instead of a truncated first line.
