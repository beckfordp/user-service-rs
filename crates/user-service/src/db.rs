// SPDX-License-Identifier: MIT

//! The service's database: the pool and the baseline migration (RUST-2.4).

use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use crate::config::PostgresConfig;

/// Connections in the pool, as the Scala service's skunk pool (`max = 10`).
const POOL_SIZE: u32 = 10;
/// How long a request waits for a connection before failing.
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(3);

/// Connects the pool (one connection is opened now, so a wrong address fails at startup).
///
/// # Errors
/// When the database can't be reached or refuses the login.
pub async fn connect(config: &PostgresConfig) -> Result<PgPool, sqlx::Error> {
    let options = PgConnectOptions::new()
        .host(&config.host)
        .port(config.port)
        .database(&config.database)
        .username(&config.user)
        .password(&config.password);
    PgPoolOptions::new()
        .max_connections(POOL_SIZE)
        .acquire_timeout(ACQUIRE_TIMEOUT)
        .connect_with(options)
        .await
}

/// Applies the embedded baseline migration (recorded in `_sqlx_migrations`).
///
/// # Errors
/// When a migration fails.
pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}
