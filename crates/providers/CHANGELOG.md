# Changelog

## 0.1.1

- Ensure postgres and redis never start a second, redundant `docker compose` invocation: when a compose file defines them they depend on the docker provider's compose-up task and only verify the service responds, removing a race between concurrent compose runs.
- Report a clear, actionable error when postgres or redis are detected without a compose service to start them, instead of a broken `docker compose up`.
- Show the tail of a failing command's output as the task error message, instead of a truncated first line.
- Verify postgres and redis on the actual host port a compose file publishes for them, instead of assuming 5432/6379, so projects mapping alternative ports are checked correctly.
## 0.1.0

- Added JavaScript package-manager providers for bun, npm and pnpm that check the binary and install dependencies.
- Added the cargo provider for Rust projects.
- Added the docker provider that brings up compose services in the background.
- Added the prisma and drizzle providers for ORM client generation.
- Added the redis and postgres providers that ensure backing services are running.
