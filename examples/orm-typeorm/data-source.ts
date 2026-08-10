import { DataSource } from "typeorm";

// TypeORM example data source. upone detects the `typeorm` dependency and runs
// `npx typeorm migration:run` against this data source after deps are installed.

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