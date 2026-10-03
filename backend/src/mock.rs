//! 本地模拟的外部接口：打印车间与快递网点。
//! 不发起任何真实网络请求，只在当前数据库连接/事务内落一条调用记录，
//! 便于演示时核对“打印门牌 / 快递改址 / 转发设置”是否真的触发，且只触发一次。

use chrono::Utc;

use crate::error::AppError;

fn db_err(e: sqlx::Error) -> AppError {
    AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        "mock_failed",
        format!("模拟接口落库失败: {e}"),
    )
}

async fn record<'e, E>(
    executor: E,
    application_id: i64,
    kind: &str,
    target: &str,
    payload: serde_json::Value,
    result: String,
) -> Result<(), AppError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    sqlx::query(
        "INSERT INTO notifications (application_id, kind, target, payload, result, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(application_id)
    .bind(kind)
    .bind(target)
    .bind(payload.to_string())
    .bind(result)
    .bind(Utc::now().to_rfc3339())
    .execute(executor)
    .await
    .map_err(db_err)?;
    Ok(())
}

#[derive(serde::Serialize)]
pub struct MockResponse {
    pub ok: bool,
    pub receipt: String,
    pub target: String,
}

/// 门牌制作单（实体门牌/楼层导视统一一个生效日）
pub async fn send_print_order<'e, E>(
    executor: E,
    application_id: i64,
    room_code: &str,
    tenant_name: &str,
    effective_date: &str,
) -> Result<MockResponse, AppError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let receipt = format!("PRT-{}-{}", application_id, effective_date.replace('-', ""));
    let payload = serde_json::json!({
        "roomCode": room_code,
        "tenantName": tenant_name,
        "effectiveDate": effective_date,
        "items": ["实体门牌", "楼层导视牌", "前台名册条目"],
    });
    record(
        executor,
        application_id,
        "print",
        "local://print-workshop/orders",
        payload,
        format!("accepted receipt={}", receipt),
    )
    .await?;
    Ok(MockResponse {
        ok: true,
        receipt,
        target: "打印车间（模拟）".to_string(),
    })
}

/// 快递地址更新通知：新地址启用、旧地址停止作为收件地址
pub async fn send_courier_update<'e, E>(
    executor: E,
    application_id: i64,
    tenant_name: &str,
    old_code: Option<&str>,
    new_code: &str,
    effective_date: &str,
) -> Result<MockResponse, AppError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let receipt = format!("CRT-{}-{}", application_id, effective_date.replace('-', ""));
    let payload = serde_json::json!({
        "tenantName": tenant_name,
        "oldAddress": old_code,
        "newAddress": new_code,
        "effectiveDate": effective_date,
        "note": "生效日起旧址不再作为新租户收件地址，过渡期内旧址快件转发至新址",
    });
    record(
        executor,
        application_id,
        "courier",
        "local://courier-station/address-update",
        payload,
        format!("accepted receipt={}", receipt),
    )
    .await?;
    Ok(MockResponse {
        ok: true,
        receipt,
        target: "快递网点（模拟）".to_string(),
    })
}

/// 旧房间快件转发设置
pub async fn send_forward_rule<'e, E>(
    executor: E,
    application_id: i64,
    old_code: &str,
    new_code: &str,
    until: &str,
) -> Result<MockResponse, AppError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let payload = serde_json::json!({
        "oldAddress": old_code,
        "forwardTo": new_code,
        "validUntil": until,
    });
    record(
        executor,
        application_id,
        "courier",
        "local://courier-station/forward-rule",
        payload,
        "forwarding active".to_string(),
    )
    .await?;
    Ok(MockResponse {
        ok: true,
        receipt: format!("FWD-{}-{}", application_id, until.replace('-', "")),
        target: "快递网点（模拟）".to_string(),
    })
}
