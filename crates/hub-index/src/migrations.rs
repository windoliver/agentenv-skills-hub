use sqlx::{Executor, PgPool};

pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(0x0A9E_7EAB_i64)
        .execute(&mut *tx)
        .await?;
    (&mut *tx)
        .execute(include_str!("../../../migrations/0001_init.sql"))
        .await?;
    tx.commit().await?;
    Ok(())
}
