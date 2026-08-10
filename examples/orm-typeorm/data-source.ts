import { DataSource } from "typeorm";

// TypeORM example data source. upone detects the `typeorm` dependency and runs
// `npx typeorm migration:run -d data-source.ts` against this data source after
// deps are installed.
//
// The port must match the host port the compose service publishes (25432 here);
// upone reads that mapping from compose.yml instead of assuming the default.

export default new DataSource({
  type: "postgres",
  host: "localhost",
  port: 25432,
  username: "upone",
  password: "upone",
  database: "upone",
  entities: ["src/entity/*.ts"],
  migrations: ["src/migration/*.ts"],
});