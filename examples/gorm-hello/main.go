package main

import (
	"gorm.io/driver/sqlite"
	"gorm.io/gorm"
)

// GORM example. upone detects the `gorm.io/gorm` dependency in go.mod; GORM is
// informational — migrations are code (`AutoMigrate`), so upone just flags the
// project and lets `go build` handle the rest.

type User struct {
	ID    uint
	Email string
}

func main() {
	db, err := gorm.Open(sqlite.Open("app.db"), &gorm.Config{})
	if err != nil {
		panic(err)
	}
	db.AutoMigrate(&User{})
}