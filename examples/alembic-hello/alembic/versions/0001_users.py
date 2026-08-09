from alembic import op
import sqlalchemy as sa

# Alembic example migration. Applied with `python -m alembic upgrade head`.

revision = "0001"
down_revision = None
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.create_table(
        "users",
        sa.Column("id", sa.Integer(), primary_key=True),
        sa.Column("email", sa.String(), nullable=False, unique=True),
    )


def downgrade() -> None:
    op.drop_table("users")