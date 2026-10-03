//! 业务核心：申请状态机、门牌占用控制、统一生效日的发布事务。
//!
//! 状态流转：
//!   submitted --approve--> approved --fabricate(ok)--> fabricated
//!   --install--> installed --accept--> accepted --publish--> published
//! 任一环节校验失败 -> blocked（附带 blocked_stage / block_reason），暂停发布。
//!
//! 关键保障：
//! * 审核占用门牌时受部分唯一索引 uq_application_active_target 硬约束，
//!   两个申请不可能同时占用同一门牌（含并发请求）。
//! * publish 用条件 UPDATE 做 CAS + published_once 幂等闸，重复发布只返回
//!   idempotent，不会二次切换、不会重复触发打印/快递模拟接口。

use chrono::{Duration, NaiveDate, Utc};
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};

use crate::error::AppError;
use crate::mock;
use crate::models::*;

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn parse_date(field: &str, s: &str) -> Result<NaiveDate, AppError> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| AppError::bad_request(format!("{field} 日期格式应为 YYYY-MM-DD")))
}

async fn record_event(
    conn: &mut sqlx::SqliteConnection,
    application_id: i64,
    from: Option<&str>,
    to: &str,
    actor: &str,
    note: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO status_events (application_id, from_status, to_status, actor, note, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(application_id)
    .bind(from)
    .bind(to)
    .bind(actor)
    .bind(note)
    .bind(now())
    .execute(&mut *conn)
    .await
    .map_err(|e| AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        "db_error",
        format!("记录状态事件失败: {e}"),
    ))?;
    Ok(())
}

async fn fetch_application(
    conn: &mut sqlx::SqliteConnection,
    id: i64,
) -> Result<Application, AppError> {
    sqlx::query_as::<_, Application>("SELECT * FROM applications WHERE id = ?1")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| AppError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "db_error",
            e.to_string(),
        ))?
        .ok_or_else(|| AppError::not_found("申请不存在"))
}

/// 把唯一索引冲突翻译成可读的门牌占用错误
fn map_constraint(e: sqlx::Error) -> AppError {
    let msg = e.to_string();
    if msg.contains("uq_application_active_target") {
        AppError::conflict(
            "room_already_reserved",
            "该门牌已被另一个进行中的申请占用，无法重复占位",
        )
    } else {
        AppError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "db_error",
            msg,
        )
    }
}

// ---------- 申请输入 ----------

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateInput {
    pub tenant_name: String,
    pub tenant_id: Option<i64>,
    pub old_room_id: Option<i64>,
    pub new_room_id: i64,
    pub lease_ref: String,
    pub lease_effective_date: String,
    pub effective_date: String,
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PatchInput {
    pub tenant_name: Option<String>,
    pub new_room_id: Option<i64>,
    pub lease_ref: Option<String>,
    pub lease_effective_date: Option<String>,
    pub effective_date: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FabricateInput {
    pub ok: bool,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallInput {
    pub installer: String,
    pub install_photo: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptInput {
    pub accepted_by: String,
}

// ---------- 查询 ----------

fn application_json(app: &Application, rooms: &[Room]) -> Value {
    let find = |id: i64| rooms.iter().find(|r| r.id == id);
    json!({
        "id": app.id,
        "code": app.code,
        "tenantId": app.tenant_id,
        "tenantName": app.tenant_name,
        "oldRoomId": app.old_room_id,
        "newRoomId": app.new_room_id,
        "oldRoomCode": app.old_room_id.and_then(find).map(|r| r.code.clone()),
        "newRoomCode": find(app.new_room_id).map(|r| r.code.clone()),
        "leaseRef": app.lease_ref,
        "leaseEffectiveDate": app.lease_effective_date,
        "effectiveDate": app.effective_date,
        "status": app.status,
        "blockReason": app.block_reason,
        "blockedStage": app.blocked_stage,
        "fabricationOk": app.fabrication_ok,
        "fabricationNote": app.fabrication_note,
        "installer": app.installer,
        "installPhoto": app.install_photo,
        "installedAt": app.installed_at,
        "acceptedBy": app.accepted_by,
        "acceptedAt": app.accepted_at,
        "publishedOnce": app.published_once,
        "publishedAt": app.published_at,
        "createdAt": app.created_at,
        "updatedAt": app.updated_at,
    })
}

async fn all_rooms(conn: &mut sqlx::SqliteConnection) -> Result<Vec<Room>, AppError> {
    sqlx::query_as::<_, Room>("SELECT * FROM rooms ORDER BY floor, code")
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| AppError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "db_error",
            e.to_string(),
        ))
}

pub async fn list_applications(pool: &SqlitePool) -> Result<Vec<Value>, AppError> {
    let mut conn = pool.acquire().await.map_err(|e| AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
    ))?;
    let apps = sqlx::query_as::<_, Application>("SELECT * FROM applications ORDER BY id")
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| AppError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
        ))?;
    let rooms = all_rooms(&mut conn).await?;
    Ok(apps.iter().map(|a| application_json(a, &rooms)).collect())
}

pub async fn get_application_detail(pool: &SqlitePool, id: i64) -> Result<Value, AppError> {
    let mut conn = pool.acquire().await.map_err(|e| AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
    ))?;
    let app = fetch_application(&mut conn, id).await?;
    let rooms = all_rooms(&mut conn).await?;
    let events = sqlx::query_as::<_, StatusEvent>(
        "SELECT * FROM status_events WHERE application_id = ?1 ORDER BY id",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
    ))?;
    let notifs = sqlx::query_as::<_, Notification>(
        "SELECT * FROM notifications WHERE application_id = ?1 ORDER BY id",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
    ))?;
    let mut body = application_json(&app, &rooms);
    body["events"] = serde_json::to_value(&events).unwrap();
    body["notifications"] = serde_json::to_value(&notifs).unwrap();
    Ok(body)
}

// ---------- 创建 ----------

pub async fn create_application(pool: &SqlitePool, input: CreateInput) -> Result<Value, AppError> {
    if input.tenant_name.trim().is_empty() {
        return Err(AppError::bad_request("租户名称不能为空"));
    }
    if input.lease_ref.trim().is_empty() {
        return Err(AppError::bad_request("请提供租约材料编号"));
    }
    parse_date("leaseEffectiveDate", &input.lease_effective_date)?;
    let effective = parse_date("effectiveDate", &input.effective_date)?;
    // 租约晚于切换日的硬拦截放在审核环节（approve），创建后可修正资料重新提交。

    let mut tx = pool.begin().await.map_err(|e| AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
    ))?;

    // 校验新门牌存在
    let new_room = sqlx::query_as::<_, Room>("SELECT * FROM rooms WHERE id = ?1")
        .bind(input.new_room_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
        ))?
        .ok_or_else(|| AppError::bad_request("目标门牌不存在"))?;

    if let Some(old_id) = input.old_room_id {
        let exists: i64 = sqlx::query("SELECT COUNT(*) AS c FROM rooms WHERE id = ?1")
            .bind(old_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| AppError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
            ))?
            .get("c");
        if exists == 0 {
            return Err(AppError::bad_request("旧地址不存在"));
        }
    }

    let seq: i64 = sqlx::query("SELECT COALESCE(MAX(id), 0) + 1 AS s FROM applications")
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
        ))?
        .get("s");
    let code = format!("CHG-{:04}-{:03}", effective.format("%Y"), seq);
    let ts = now();

    let result = sqlx::query(
        "INSERT INTO applications
            (code, tenant_id, tenant_name, old_room_id, new_room_id, lease_ref,
             lease_effective_date, effective_date, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'submitted', ?9, ?9)",
    )
    .bind(&code)
    .bind(input.tenant_id)
    .bind(input.tenant_name.trim())
    .bind(input.old_room_id)
    .bind(new_room.id)
    .bind(input.lease_ref.trim())
    .bind(input.lease_effective_date)
    .bind(input.effective_date)
    .bind(&ts)
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
    ))?;
    let id = result.last_insert_rowid();
    record_event(&mut tx, id, None, "submitted", "租户", Some("提交门牌变更申请")).await?;
    tx.commit().await.map_err(|e| AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
    ))?;

    get_application_detail(pool, id).await
}

// ---------- 审核：占用目标门牌 ----------

async fn block_application(
    conn: &mut sqlx::SqliteConnection,
    app: &Application,
    stage: &str,
    code: &str,
    reason: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE applications
            SET status = 'blocked', blocked_stage = ?1, block_reason = ?2, updated_at = ?3
          WHERE id = ?4",
    )
    .bind(stage)
    .bind(reason)
    .bind(now())
    .bind(app.id)
    .execute(&mut *conn)
    .await
    .map_err(|e| AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
    ))?;
    record_event(
        conn,
        app.id,
        Some(&app.status),
        "blocked",
        "物业",
        Some(&format!("[{code}] {reason}")),
    )
    .await?;
    Ok(())
}

pub async fn approve(pool: &SqlitePool, id: i64) -> Result<Value, AppError> {
    let mut tx = pool.begin().await.map_err(|e| AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
    ))?;
    let app = fetch_application(&mut tx, id).await?;

    if app.status != "submitted" {
        return Err(AppError::conflict(
            "invalid_transition",
            format!("当前状态为 {}，仅 submitted 状态可审核", app.status),
        ));
    }

    let lease = parse_date("leaseEffectiveDate", &app.lease_effective_date)?;
    let effective = parse_date("effectiveDate", &app.effective_date)?;

    // 校验 1：租约须在切换日（含）之前生效
    if lease > effective {
        let reason = format!(
            "租约 {} 尚未生效，晚于统一切换日 {}，暂停发布",
            app.lease_effective_date, app.effective_date
        );
        block_application(&mut tx, &app, "approval", "lease_not_effective", &reason).await?;
        tx.commit().await.map_err(|e| AppError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
        ))?;
        return Err(AppError::conflict("lease_not_effective", reason));
    }

    let target = sqlx::query_as::<_, Room>("SELECT * FROM rooms WHERE id = ?1")
        .bind(app.new_room_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
        ))?;

    // 校验 2：过渡期中的旧门牌不得再作为新租户地址
    if target.status == ROOM_TRANSITION
        && target.forward_until.as_deref().map_or(true, |u| {
            NaiveDate::parse_from_str(u, "%Y-%m-%d").map_or(true, |d| d >= Utc::now().date_naive())
        })
    {
        let reason = format!(
            "门牌 {} 处于快件过渡转发期（至 {}），不能再用作新租户地址",
            target.code,
            target.forward_until.as_deref().unwrap_or("?")
        );
        block_application(&mut tx, &app, "approval", "room_in_transition", &reason).await?;
        tx.commit().await.map_err(|e| AppError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
        ))?;
        return Err(AppError::conflict("room_in_transition", reason));
    }

    // 校验 3：门牌已被预约（reserved）或记录了其他进行中申请
    if target.status == ROOM_RESERVED
        || target
            .incoming_application_id
            .is_some_and(|other| other != app.id)
    {
        let other = target.incoming_application_id.unwrap_or(-1);
        let reason = format!("门牌 {} 已被申请 {} 预约占用", target.code, other);
        block_application(&mut tx, &app, "approval", "room_already_reserved", &reason).await?;
        tx.commit().await?;
        return Err(AppError::conflict("room_already_reserved", reason));
    }

    // 校验 4：门牌仍被其他租户实际占用
    if target.status == ROOM_OCCUPIED && target.active_tenant_id != app.tenant_id {
        let current = sqlx::query_as::<_, Tenant>("SELECT * FROM tenants WHERE id = ?1")
            .bind(target.active_tenant_id)
            .fetch_optional(&mut *tx)
            .await?
            .map(|t| t.name)
            .unwrap_or_else(|| "其他租户".to_string());
        let reason = format!("门牌 {} 已被「{}」占用，无法分配", target.code, current);
        block_application(&mut tx, &app, "approval", "room_occupied", &reason).await?;
        tx.commit().await?;
        return Err(AppError::conflict("room_occupied", reason));
    }

    // 通过：在同一事务内占位（部分唯一索引兜底并发冲突）
    if target.status == ROOM_VACANT {
        sqlx::query("UPDATE rooms SET status = 'reserved', incoming_application_id = ?1 WHERE id = ?2")
            .bind(app.id)
            .bind(target.id)
            .execute(&mut *tx)
            .await
            .map_err(map_constraint)?;
    } else {
        // occupied 且 active_tenant 就是本租户（仅更名）或过渡已到期：登记预约即可
        sqlx::query("UPDATE rooms SET incoming_application_id = ?1 WHERE id = ?2")
            .bind(app.id)
            .bind(target.id)
            .execute(&mut *tx)
            .await
            .map_err(map_constraint)?;
    }
    sqlx::query("UPDATE applications SET status = 'approved', block_reason = NULL, blocked_stage = NULL, updated_at = ?1 WHERE id = ?2")
        .bind(now())
        .bind(app.id)
        .execute(&mut *tx)
        .await
        .map_err(map_constraint)?;
    record_event(&mut tx, app.id, Some("submitted"), "approved", "物业",
        Some(&format!("审核通过，门牌 {} 已预约", target.code))).await?;
    tx.commit().await.map_err(map_constraint)?;

    get_application_detail(pool, id).await
}

// ---------- 门牌制作（合格才继续；同时向模拟打印车间下单） ----------

pub async fn fabricate(
    pool: &SqlitePool,
    id: i64,
    input: FabricateInput,
) -> Result<Value, AppError> {
    let mut tx = pool.begin().await.map_err(|e| AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
    ))?;
    let app = fetch_application(&mut tx, id).await?;

    // 允许对“制作有误”被拦下的申请重新制作
    let retry = app.status == "blocked" && app.blocked_stage.as_deref() == Some("fabrication");
    if app.status != "approved" && !retry {
        return Err(AppError::conflict(
            "invalid_transition",
            format!("当前状态为 {}，不能录入制作结果", app.status),
        ));
    }

    if !input.ok {
        let reason = input
            .note
            .clone()
            .unwrap_or_else(|| "门牌制作有误（名称/门牌不符），暂停发布".to_string());
        block_application(&mut tx, &app, "fabrication", "fabrication_failed", &reason).await?;
        tx.commit().await.map_err(|e| AppError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
        ))?;
        return Err(AppError::conflict("fabrication_failed", reason));
    }

    let target = sqlx::query_as::<_, Room>("SELECT * FROM rooms WHERE id = ?1")
        .bind(app.new_room_id)
        .fetch_one(&mut *tx)
        .await?;

    sqlx::query(
        "UPDATE applications
            SET status = 'fabricated', fabrication_ok = 1, fabrication_note = ?1,
                block_reason = NULL, blocked_stage = NULL, updated_at = ?2
          WHERE id = ?3",
    )
    .bind(input.note.clone().unwrap_or_else(|| "制作合格".to_string()))
    .bind(now())
    .bind(app.id)
    .execute(&mut *tx)
    .await?;
    record_event(&mut tx, app.id, Some(&app.status), "fabricated", "制作商",
        Some("门牌及楼层导视制作合格")).await?;

    // 本地模拟接口：向打印车间下制作单
    mock::send_print_order(&mut *tx, app.id, &target.code, &app.tenant_name, &app.effective_date)
        .await?;

    tx.commit().await.map_err(|e| AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
    ))?;
    get_application_detail(pool, id).await
}

// ---------- 安装（保存安装人与安装照片） ----------

pub async fn install(pool: &SqlitePool, id: i64, input: InstallInput) -> Result<Value, AppError> {
    if input.installer.trim().is_empty() || input.install_photo.trim().is_empty() {
        return Err(AppError::bad_request("安装人与安装照片不能为空"));
    }
    let mut tx = pool.begin().await.map_err(|e| AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
    ))?;
    let app = fetch_application(&mut tx, id).await?;
    if app.status != "fabricated" {
        return Err(AppError::conflict(
            "invalid_transition",
            format!("当前状态为 {}，制作合格后才能安装", app.status),
        ));
    }
    let ts = now();
    sqlx::query(
        "UPDATE applications
            SET status = 'installed', installer = ?1, install_photo = ?2,
                installed_at = ?3, updated_at = ?3
          WHERE id = ?4",
    )
    .bind(input.installer.trim())
    .bind(input.install_photo.trim())
    .bind(&ts)
    .bind(app.id)
    .execute(&mut *tx)
    .await?;
    record_event(&mut tx, app.id, Some("fabricated"), "installed", input.installer.trim(),
        Some(&format!("实体门牌安装完成，照片：{}", input.install_photo))).await?;
    tx.commit().await.map_err(|e| AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
    ))?;
    get_application_detail(pool, id).await
}

// ---------- 验收（保存验收人） ----------

pub async fn accept(pool: &SqlitePool, id: i64, input: AcceptInput) -> Result<Value, AppError> {
    if input.accepted_by.trim().is_empty() {
        return Err(AppError::bad_request("验收人不能为空"));
    }
    let mut tx = pool.begin().await.map_err(|e| AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
    ))?;
    let app = fetch_application(&mut tx, id).await?;
    if app.status != "installed" {
        return Err(AppError::conflict(
            "invalid_transition",
            format!("当前状态为 {}，安装完成后才能验收", app.status),
        ));
    }
    let ts = now();
    sqlx::query(
        "UPDATE applications
            SET status = 'accepted', accepted_by = ?1, accepted_at = ?2, updated_at = ?2
          WHERE id = ?3",
    )
    .bind(input.accepted_by.trim())
    .bind(&ts)
    .bind(app.id)
    .execute(&mut *tx)
    .await?;
    record_event(&mut tx, app.id, Some("installed"), "accepted", input.accepted_by.trim(),
        Some("门牌、楼层导视与前台名册验收通过")).await?;
    tx.commit().await.map_err(|e| AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
    ))?;
    get_application_detail(pool, id).await
}

// ---------- 发布：统一生效日切换（幂等！） ----------

pub struct PublishOutcome {
    pub switched: bool,
    pub detail: Value,
}

pub async fn publish(pool: &SqlitePool, id: i64) -> Result<PublishOutcome, AppError> {
    let mut tx = pool.begin().await.map_err(|e| AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
    ))?;
    let app = fetch_application(&mut tx, id).await?;

    // 幂等闸：已经发布过的申请直接返回，不产生任何第二次切换/通知/事件
    if app.published_once == 1 || app.status == "published" {
        tx.rollback().await.ok();
        let detail = get_application_detail(pool, id).await?;
        return Ok(PublishOutcome { switched: false, detail });
    }
    if app.status != "accepted" {
        return Err(AppError::conflict(
            "not_ready",
            format!("申请尚未完成验收（当前 {}），不能发布", app.status),
        ));
    }

    // CAS：只有仍是 accepted 且未发布时才能抢到切换权（防并发双重发布）
    let ts = now();
    let affected = sqlx::query(
        "UPDATE applications
            SET status = 'published', published_once = 1, published_at = ?1, updated_at = ?1
          WHERE id = ?2 AND status = 'accepted' AND published_once = 0",
    )
    .bind(&ts)
    .bind(app.id)
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
    ))?
    .rows_affected();
    if affected == 0 {
        // 已被并发请求抢先发布
        tx.rollback().await.ok();
        let detail = get_application_detail(pool, id).await?;
        return Ok(PublishOutcome { switched: false, detail });
    }

    // 解析最终租户（新租户在发布时建档）
    let tenant_id = match app.tenant_id {
        Some(tid) => tid,
        None => {
            let r = sqlx::query(
                "INSERT INTO tenants (name, current_room_id, active) VALUES (?1, ?2, 1)",
            )
            .bind(&app.tenant_name)
            .bind(app.new_room_id)
            .execute(&mut *tx)
            .await?;
            let tid = r.last_insert_rowid();
            sqlx::query("UPDATE applications SET tenant_id = ?1 WHERE id = ?2")
                .bind(tid)
                .bind(app.id)
                .execute(&mut *tx)
                .await?;
            tid
        }
    };

    // 既有租户更名
    sqlx::query("UPDATE tenants SET name = ?1 WHERE id = ?2")
        .bind(&app.tenant_name)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await?;

    let new_room = sqlx::query_as::<_, Room>("SELECT * FROM rooms WHERE id = ?1")
        .bind(app.new_room_id)
        .fetch_one(&mut *tx)
        .await?;

    // 旧地址：迁入过渡期（仅转发快件，不再是租户地址）。同名同址（仅更名）不处理。
    if let Some(old_id) = app.old_room_id {
        if old_id != app.new_room_id {
            sqlx::query(
                "UPDATE rooms
                    SET status = 'transition', active_tenant_id = NULL,
                        forward_to_room_id = ?1,
                        forward_until = ?2,
                        forward_tenant_name = ?3,
                        incoming_application_id = NULL
                  WHERE id = ?4",
            )
            .bind(app.new_room_id)
            .bind((NaiveDate::parse_from_str(&app.effective_date, "%Y-%m-%d").unwrap()
                + Duration::days(14))
            .format("%Y-%m-%d")
            .to_string())
            .bind(&app.tenant_name)
            .bind(old_id)
            .execute(&mut *tx)
            .await?;
        }
    }

    // 新地址：正式入住，清除预约与转发痕迹
    sqlx::query(
        "UPDATE rooms
            SET status = 'occupied', active_tenant_id = ?1,
                incoming_application_id = NULL,
                forward_to_room_id = NULL, forward_until = NULL, forward_tenant_name = NULL
          WHERE id = ?2",
    )
    .bind(tenant_id)
    .bind(new_room.id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("UPDATE tenants SET current_room_id = ?1 WHERE id = ?2")
        .bind(new_room.id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await?;

    record_event(&mut tx, app.id, Some("accepted"), "published", "物业", Some(&format!(
        "切换日 {} 生效：实体门牌、楼层导视、前台名册与快递地址统一更新", app.effective_date
    ))).await?;

    // 本地模拟接口：快递地址更新 + 旧址转发规则（只在真正切换时触发一次）
    let old_code_real = if let Some(oid) = app.old_room_id.filter(|&o| o != app.new_room_id) {
        sqlx::query_as::<_, Room>("SELECT * FROM rooms WHERE id = ?1")
            .bind(oid)
            .fetch_optional(&mut *tx)
            .await?
            .map(|r| r.code)
    } else {
        None
    };
    mock::send_courier_update(
        &mut *tx,
        app.id,
        &app.tenant_name,
        old_code_real.as_deref(),
        &new_room.code,
        &app.effective_date,
    )
    .await?;
    if let Some(ref oc) = old_code_real {
        let until = (NaiveDate::parse_from_str(&app.effective_date, "%Y-%m-%d").unwrap()
            + Duration::days(14))
        .format("%Y-%m-%d")
        .to_string();
        mock::send_forward_rule(&mut *tx, app.id, oc, &new_room.code, &until).await?;
    }

    tx.commit().await.map_err(|e| AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
    ))?;

    let detail = get_application_detail(pool, id).await?;
    Ok(PublishOutcome { switched: true, detail })
}

// ---------- 资料修正后重新提交 ----------

pub async fn resubmit(pool: &SqlitePool, id: i64, patch: PatchInput) -> Result<Value, AppError> {
    let mut tx = pool.begin().await.map_err(|e| AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
    ))?;
    let app = fetch_application(&mut tx, id).await?;
    if app.status != "submitted" && app.status != "blocked" {
        return Err(AppError::conflict(
            "invalid_transition",
            format!("当前状态为 {}，资料已锁定", app.status),
        ));
    }
    if let Some(d) = &patch.lease_effective_date {
        parse_date("leaseEffectiveDate", d)?;
    }
    if let Some(d) = &patch.effective_date {
        parse_date("effectiveDate", d)?;
    }
    if let Some(rid) = patch.new_room_id {
        let n: i64 = sqlx::query("SELECT COUNT(*) AS c FROM rooms WHERE id = ?1")
            .bind(rid)
            .fetch_one(&mut *tx)
            .await?
            .get("c");
        if n == 0 {
            return Err(AppError::bad_request("目标门牌不存在"));
        }
    }

    sqlx::query(
        "UPDATE applications SET
            tenant_name = COALESCE(?1, tenant_name),
            new_room_id = COALESCE(?2, new_room_id),
            lease_ref   = COALESCE(?3, lease_ref),
            lease_effective_date = COALESCE(?4, lease_effective_date),
            effective_date = COALESCE(?5, effective_date),
            status = 'submitted', block_reason = NULL, blocked_stage = NULL,
            updated_at = ?6
          WHERE id = ?7",
    )
    .bind(patch.tenant_name)
    .bind(patch.new_room_id)
    .bind(patch.lease_ref)
    .bind(patch.lease_effective_date)
    .bind(patch.effective_date)
    .bind(now())
    .bind(app.id)
    .execute(&mut *tx)
    .await?;
    record_event(&mut tx, app.id, Some(&app.status), "submitted", "租户",
        Some("修正资料后重新提交审核")).await?;
    tx.commit().await.map_err(|e| AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
    ))?;
    get_application_detail(pool, id).await
}

// ---------- 楼层导视 / 前台视图 ----------

pub async fn guide(pool: &SqlitePool) -> Result<Value, AppError> {
    let mut conn = pool.acquire().await.map_err(|e| AppError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
    ))?;
    let rooms = all_rooms(&mut conn).await?;
    let tenants = sqlx::query_as::<_, Tenant>("SELECT * FROM tenants")
        .fetch_all(&mut *conn)
        .await?;
    let apps = sqlx::query_as::<_, Application>("SELECT * FROM applications ORDER BY id")
        .fetch_all(&mut *conn)
        .await?;

    let room_json = |r: &Room| -> Value {
        let tenant_name = r
            .active_tenant_id
            .and_then(|tid| tenants.iter().find(|t| t.id == tid))
            .map(|t| t.name.clone());
        let incoming = r.incoming_application_id.and_then(|aid| {
            apps.iter().find(|a| a.id == aid && a.status != "published").map(|a| json!({
                "applicationCode": a.code,
                "tenantName": a.tenant_name,
                "effectiveDate": a.effective_date,
                "applicationStatus": a.status,
            }))
        });
        let forward_to = r
            .forward_to_room_id
            .and_then(|fid| rooms.iter().find(|x| x.id == fid))
            .map(|x| x.code.clone());
        json!({
            "id": r.id,
            "floor": r.floor,
            "code": r.code,
            "label": r.label,
            "status": r.status,
            "tenantName": tenant_name,
            "incoming": incoming,
            "forwardToRoomCode": forward_to,
            "forwardUntil": r.forward_until,
            "forwardTenantName": r.forward_tenant_name,
        })
    };

    // 进行中变更：前台页面同时展示旧址、新址与切换日期
    let changes: Vec<Value> = apps
        .iter()
        .filter(|a| {
            ["approved", "fabricated", "installed", "accepted", "published"]
                .contains(&a.status.as_str())
        })
        .map(|a| {
            let find_room = |id: Option<i64>| {
                id.and_then(|rid| rooms.iter().find(|r| r.id == rid)).map(|r| r.code.clone())
            };
            json!({
                "applicationId": a.id,
                "applicationCode": a.code,
                "tenantName": a.tenant_name,
                "oldRoomCode": find_room(a.old_room_id),
                "newRoomCode": find_room(Some(a.new_room_id)),
                "effectiveDate": a.effective_date,
                "status": a.status,
            })
        })
        .collect();

    let mut floors: Vec<String> = rooms.iter().map(|r| r.floor.clone()).collect();
    floors.sort();
    floors.dedup();
    let floors_json: Vec<Value> = floors
        .iter()
        .map(|f| {
            json!({
                "floor": f,
                "rooms": rooms.iter().filter(|r| &r.floor == f).map(room_json).collect::<Vec<_>>(),
            })
        })
        .collect();

    Ok(json!({ "floors": floors_json, "changes": changes }))
}

pub async fn list_notifications(pool: &SqlitePool) -> Result<Vec<Notification>, AppError> {
    sqlx::query_as::<_, Notification>("SELECT * FROM notifications ORDER BY id DESC")
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
        ))
}

pub async fn list_tenants(pool: &SqlitePool) -> Result<Vec<Tenant>, AppError> {
    sqlx::query_as::<_, Tenant>("SELECT * FROM tenants WHERE active = 1 ORDER BY id")
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
        ))
}

pub async fn list_rooms(pool: &SqlitePool) -> Result<Vec<Room>, AppError> {
    sqlx::query_as::<_, Room>("SELECT * FROM rooms ORDER BY floor, code")
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string(),
        ))
}
