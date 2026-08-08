---
crate: upone-providers
bump: minor
---

Added the `mysql` provider (detects `mysql`/`mariadb` in docker-compose or a `mysql://`/`mariadb://` `DATABASE_URL`, ensures the service responds on `localhost:3306`).
Added the `mongo` provider (detects `mongo`/`mongodb` in docker-compose or a `mongodb://` URI via `MONGODB_URI`/`MONGO_URI`/`DATABASE_URL`, ensures the service responds on `localhost:27017`).
Added the `sqlite` provider (detects a `sqlite://` `DATABASE_URL` or an ORM config targeting sqlite; creates the database file if missing — there is no server to start).
Added the `mongoose` provider (recognizes the MongoDB ODM via the `mongoose` dependency; informational).