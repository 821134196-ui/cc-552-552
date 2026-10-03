use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// 建立连接池并执行迁移。`url` 形如 `sqlite:doorplate.db` 或 `sqlite::memory:`。
pub async fn init_pool(url: &str) -> Result<SqlitePool, sqlx::Error> {
    let opts = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        .foreign_keys(true)
        // 并发写（如两个发布请求竞争）时等待写锁而非立即报 database is locked
        .busy_timeout(std::time::Duration::from_secs(5));
    // 内存库每个连接都是独立数据库，必须单连接
    let max_connections = if url.contains(":memory:") { 1 } else { 8 };
    let pool = SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(opts)
        .await?;
    MIGRATOR.run(&pool).await?;
    Ok(pool)
}

/// 首次启动时写入演示数据（仅当 tenants 为空）。
pub async fn seed_if_empty(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tenants")
        .fetch_one(pool)
        .await?;
    if count > 0 {
        return Ok(());
    }

    let tenants = [
        ("星辰科技有限公司", "张工 138-0000-0001"),
        ("蓝海咨询有限公司", "李经理 138-0000-0002"),
        ("云图设计工作室", "王设计 138-0000-0003"),
        ("绿洲餐饮管理公司", "赵店长 138-0000-0004"),
        ("恒信物流有限公司", "陈调度 138-0000-0005"),
    ];
    for (name, contact) in tenants {
        sqlx::query("INSERT INTO tenants (name, contact) VALUES (?, ?)")
            .bind(name)
            .bind(contact)
            .execute(pool)
            .await?;
    }

    // (floor, room, status, tenant_id)
    let doorplates: [(i64, &str, &str, Option<i64>); 10] = [
        (1, "101", "occupied", Some(1)), // 星辰科技
        (1, "102", "available", None),
        (1, "103", "occupied", Some(2)), // 蓝海咨询
        (2, "201", "occupied", Some(3)), // 云图设计
        (2, "202", "available", None),
        (2, "203", "available", None),
        (3, "301", "available", None),
        (3, "302", "occupied", Some(4)), // 绿洲餐饮
        (3, "303", "occupied", Some(5)), // 恒信物流
        (3, "305", "available", None),
    ];
    for (floor, room, status, tenant_id) in doorplates {
        sqlx::query("INSERT INTO doorplates (floor, room, status, tenant_id) VALUES (?, ?, ?, ?)")
            .bind(floor)
            .bind(room)
            .bind(status)
            .bind(tenant_id)
            .execute(pool)
            .await?;
    }

    // 一条待审核的示例申请：星辰科技 101 -> 202，2026-10-10 切换
    sqlx::query(
        "INSERT INTO applications
         (tenant_id, change_type, old_doorplate_id, target_doorplate_id,
          lease_no, lease_start, lease_end, material, effective_date, status, created_at, updated_at)
         VALUES (1, 'move', 1, 5, 'ZL-2026-1001', '2026-10-01', '2028-09-30',
                 'lease-2026-1001.pdf', '2026-10-10', 'submitted',
                 datetime('now', 'localtime'), datetime('now', 'localtime'))",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO state_events (application_id, from_status, to_status, reason, actor, created_at)
         VALUES (1, NULL, 'submitted', '租户在线提交', '租户', datetime('now', 'localtime'))",
    )
    .execute(pool)
    .await?;

    tracing::info!("已写入演示种子数据");
    Ok(())
}
