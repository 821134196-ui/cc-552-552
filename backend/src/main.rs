mod error;
mod handlers;
mod mock;
mod models;
mod service;

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tower_http::services::ServeDir;

const MIGRATION: &str = include_str!("../migrations/0001_init.sql");

/// 演示用初始数据：三层楼、6 个门牌、3 家在租租户。
const SEED: &str = r#"
INSERT INTO tenants (id, name, current_room_id, active) VALUES
  (1, '云岚科技有限公司', NULL, 1),
  (2, '北辰工业设计事务所', NULL, 1),
  (3, '星桥文化传媒工作室', NULL, 1);

INSERT INTO rooms (id, floor, code, label, status, active_tenant_id) VALUES
  (1, '2F', '201', '电梯口西侧', 'occupied', 3),
  (2, '2F', '202', '电梯口东侧', 'vacant',   NULL),
  (3, '3F', '301', '电梯口西侧', 'occupied', 1),
  (4, '3F', '302', '电梯口东侧', 'occupied', 2),
  (5, '4F', '401', '电梯口西侧', 'vacant',   NULL),
  (6, '4F', '402', '电梯口东侧', 'vacant',   NULL);

UPDATE tenants SET current_room_id = 3 WHERE id = 1;
UPDATE tenants SET current_room_id = 4 WHERE id = 2;
UPDATE tenants SET current_room_id = 1 WHERE id = 3;
"#;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = std::env::var("DOORPLATE_DB").unwrap_or_else(|_| "data/doorplate.db".to_string());
    let frontend_dir = std::env::var("FRONTEND_DIR").unwrap_or_else(|_| "frontend/dist".to_string());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    if let Some(parent) = Path::new(&db_path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await?;

    sqlx::raw_sql(MIGRATION).execute(&pool).await?;

    let seeded: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rooms")
        .fetch_one(&pool)
        .await?;
    if seeded == 0 {
        let mut tx = pool.begin().await?;
        sqlx::raw_sql(SEED).execute(&mut *tx).await?;
        tx.commit().await?;
        println!("已写入演示种子数据");
    }

    let index_html = std::fs::read_to_string(
        Path::new(&frontend_dir).join("index.html"),
    )?;

    let app = handlers::router(pool).fallback_service(
        // 静态资源由 ServeDir 处理；不存在的路径回退到 index.html（SPA），返回 200。
        ServeDir::new(&frontend_dir).fallback(axum::routing::get(move || {
            let html = index_html.clone();
            async move { axum::response::Html(html) }
        })),
    );

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    println!("门牌变更服务已启动: http://127.0.0.1:{port}");
    println!("  数据库: {db_path}");
    println!("  前端目录: {frontend_dir}");

    let server = axum::serve(listener, app.into_make_service());
    server.await?;
    Ok(())
}
