module.exports = {
  development: {
    client: "pg",
    connection: {
      host: "127.0.0.1",
      port: 5432,
      user: "upone",
      password: "upone",
      database: "upone",
    },
    migrations: {
      directory: "./migrations",
    },
  },
};