from sqlalchemy import create_engine

# SQLAlchemy example. upone detects `sqlalchemy` in the project manifests;
# SQLAlchemy is informational — real migrations are delegated to Alembic, so
# upone just flags the ORM and leaves schema changes to the `alembic` provider
# (or the app itself).

engine = create_engine("sqlite:///./app.db")
print(engine.url.database)