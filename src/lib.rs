pub mod db;
pub mod error;
pub mod models;
pub mod routes;
pub mod service;

use sqlx::sqlite::SqlitePool;

pub fn build_router(pool: SqlitePool) -> axum::Router {
    routes::router(pool)
}
