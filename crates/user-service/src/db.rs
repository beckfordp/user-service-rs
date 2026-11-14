// SPDX-License-Identifier: MIT

//! The service's database: the pool and the baseline migration (RUST-2.4).
//! The connection is gluon-rs `service-basics::postgres`.

use sqlx::PgPool;

use crate::config::PostgresConfig;

/// Connections in the pool, as the Scala service's skunk pool (`max = 10`).
const POOL_SIZE: u32 = 10;

/// Connects the pool (one connection is opened now, so a wrong address fails at startup).
///
/// # Errors
/// When the database can't be reached or refuses the login.
pub async fn connect(config: &PostgresConfig) -> Result<PgPool, sqlx::Error> {
    service_basics::postgres::connect(config, POOL_SIZE).await
}

/// Applies the embedded baseline migration (recorded in `_sqlx_migrations`).
///
/// # Errors
/// When a migration fails.
pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}
