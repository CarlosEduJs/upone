import { DataTypes } from "sequelize";

// Sequelize example migration. upone detects the `sequelize` dependency and
// runs `npx sequelize-cli db:migrate` to apply pending migrations.

module.exports = {
  async up(queryInterface) {
    await queryInterface.createTable("Users", {
      id: { type: DataTypes.INTEGER, primaryKey: true, autoIncrement: true },
      email: { type: DataTypes.STRING, allowNull: false, unique: true },
      createdAt: { type: DataTypes.DATE, allowNull: false },
      updatedAt: { type: DataTypes.DATE, allowNull: false },
    });
  },
  async down(queryInterface) {
    await queryInterface.dropTable("Users");
  },
};