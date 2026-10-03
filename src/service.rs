use crate::error::ApiError;
use crate::models::*;
use chrono::{Duration, Local, NaiveDate};
use serde_json::json;
use sqlx::sqlite::SqlitePool;
use sqlx::{Row, Sqlite, Transaction};

// ---------- 时间工具 ----------

/// 开启一个立即获取写锁的事务：并发写时后者等待 busy_timeout，避免锁升级冲突。
async fn begin_write(pool: &SqlitePool) -> Result<Transaction<'_, Sqlite>, ApiError> {
    Ok(pool.begin_with("BEGIN IMMEDIATE").await?)
}

pub fn now() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn valid_date(s: &str) -> bool {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}

fn plus_days(date: &str, days: i64) -> String {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|d| (d + Duration::days(days)).format("%Y-%m-%d").to_string())
        .unwrap_or_else(|_| date.to_string())
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db_err) if db_err.is_unique_violation())
}

// ---------- 通用读取 ----------

async fn fetch_application_tx(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
) -> Result<Application, ApiError> {
    sqlx::query_as::<_, Application>("SELECT * FROM applications WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("申请 #{id} 不存在")))
}

async fn fetch_doorplate_tx(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
) -> Result<Doorplate, ApiError> {
    sqlx::query_as::<_, Doorplate>("SELECT * FROM doorplates WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("门牌 #{id} 不存在")))
}

const VIEW_SELECT: &str = r#"
    SELECT a.id, a.tenant_id, t.name AS tenant_name, a.change_type, a.new_name,
           a.old_doorplate_id, (od.floor || 'F ' || od.room) AS old_label,
           a.target_doorplate_id, (td.floor || 'F ' || td.room) AS target_label,
           a.lease_no, a.lease_start, a.lease_end, a.material, a.effective_date,
           a.status, a.pause_reason, a.install_photo, a.accepted_by, a.published_at,
           a.created_at, a.updated_at
    FROM applications a
    JOIN tenants t ON t.id = a.tenant_id
    LEFT JOIN doorplates od ON od.id = a.old_doorplate_id
    LEFT JOIN doorplates td ON td.id = a.target_doorplate_id
"#;

pub async fn get_application_view(pool: &SqlitePool, id: i64) -> Result<ApplicationView, ApiError> {
    sqlx::query_as::<_, ApplicationView>(&format!("{VIEW_SELECT} WHERE a.id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("申请 #{id} 不存在")))
}

pub async fn list_applications(
    pool: &SqlitePool,
    status: Option<String>,
) -> Result<Vec<ApplicationView>, ApiError> {
    let rows = match status {
        Some(s) => {
            sqlx::query_as::<_, ApplicationView>(
                &format!("{VIEW_SELECT} WHERE a.status = ? ORDER BY a.id DESC"),
            )
            .bind(s)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, ApplicationView>(&format!("{VIEW_SELECT} ORDER BY a.id DESC"))
                .fetch_all(pool)
                .await?
        }
    };
    Ok(rows)
}

pub async fn get_detail(pool: &SqlitePool, id: i64) -> Result<ApplicationDetail, ApiError> {
    let application = get_application_view(pool, id).await?;
    let events = sqlx::query_as::<_, StateEvent>(
        "SELECT * FROM state_events WHERE application_id = ? ORDER BY id",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let print_jobs =
        sqlx::query_as::<_, PrintJob>("SELECT * FROM print_jobs WHERE application_id = ? ORDER BY id")
            .bind(id)
            .fetch_all(pool)
            .await?;
    let courier_notifications = sqlx::query_as::<_, CourierNotification>(
        "SELECT * FROM courier_notifications WHERE application_id = ? ORDER BY id",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(ApplicationDetail {
        application,
        events,
        print_jobs,
        courier_notifications,
    })
}

// ---------- 门牌规则 ----------

/// 若门牌不可作为目标，返回拦截原因；否则 None。
/// 规则：占用中（他人）→ 拦截；转发过渡期内 → 拦截（旧址不可再分配给新租户）；空闲/过期 → 放行。
fn doorplate_block_reason(dp: &Doorplate, tenant_id: i64, today: &str) -> Option<String> {
    match dp.status.as_str() {
        s if s == door_status::AVAILABLE => None,
        s if s == door_status::OCCUPIED => {
            if dp.tenant_id == Some(tenant_id) {
                None // 自己的现址（更名场景）
            } else {
                Some(format!("目标门牌 {} 已被占用", dp.label()))
            }
        }
        s if s == door_status::FORWARDING => match &dp.forwarding_until {
            Some(until) if until.as_str() >= today => Some(format!(
                "目标门牌 {} 处于旧址转发过渡期（截止 {}），不可再分配给新租户",
                dp.label(),
                until
            )),
            _ => None, // 过渡期已结束，可重新分配
        },
        other => Some(format!("门牌 {} 状态异常：{other}", dp.label())),
    }
}

/// 事务内状态切换：以当前状态为守卫条件，并发下只有一方成功。
async fn set_status(
    tx: &mut Transaction<'_, Sqlite>,
    app: &Application,
    to: &str,
    reason: Option<&str>,
    actor: &str,
) -> Result<(), ApiError> {
    let now = now();
    let res = sqlx::query("UPDATE applications SET status = ?, updated_at = ? WHERE id = ? AND status = ?")
        .bind(to)
        .bind(&now)
        .bind(app.id)
        .bind(&app.status)
        .execute(&mut **tx)
        .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::conflict(format!(
            "申请 #{} 状态已变化（原 {}），操作冲突，请刷新后重试",
            app.id, app.status
        )));
    }
    sqlx::query(
        "INSERT INTO state_events (application_id, from_status, to_status, reason, actor, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(app.id)
    .bind(&app.status)
    .bind(to)
    .bind(reason)
    .bind(actor)
    .bind(&now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// 事务内暂停：记录原因与恢复目标状态。
async fn pause_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    app: &Application,
    reason: &str,
) -> Result<(), ApiError> {
    let now = now();
    sqlx::query(
        "UPDATE applications SET status = 'paused', pause_reason = ?, resume_status = ?, updated_at = ?
         WHERE id = ? AND status = ?",
    )
    .bind(reason)
    .bind(&app.status)
    .bind(&now)
    .bind(app.id)
    .bind(&app.status)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO state_events (application_id, from_status, to_status, reason, actor, created_at)
         VALUES (?, ?, 'paused', ?, '系统', ?)",
    )
    .bind(app.id)
    .bind(&app.status)
    .bind(reason)
    .bind(&now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// ---------- 提交申请 ----------

pub async fn submit_application(
    pool: &SqlitePool,
    input: SubmitApplication,
) -> Result<ApplicationView, ApiError> {
    if !valid_date(&input.lease_start) {
        return Err(ApiError::bad_request("租约起始日格式应为 YYYY-MM-DD"));
    }
    if !valid_date(&input.effective_date) {
        return Err(ApiError::bad_request("生效日格式应为 YYYY-MM-DD"));
    }
    if let Some(end) = &input.lease_end {
        if !valid_date(end) {
            return Err(ApiError::bad_request("租约截止日格式应为 YYYY-MM-DD"));
        }
    }
    if input.lease_no.trim().is_empty() {
        return Err(ApiError::bad_request("请填写租约编号"));
    }

    let mut tx = begin_write(pool).await?;

    let tenant = sqlx::query_as::<_, Tenant>("SELECT * FROM tenants WHERE id = ?")
        .bind(input.tenant_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| ApiError::not_found("租户不存在"))?;

    let today = today();
    let (new_name, old_id, target_id) = match input.change_type.as_str() {
        "move" => {
            let old_id = input
                .old_doorplate_id
                .ok_or_else(|| ApiError::bad_request("搬迁申请必须选择旧址门牌"))?;
            let old = fetch_doorplate_tx(&mut tx, old_id).await?;
            if old.tenant_id != Some(tenant.id) {
                return Err(ApiError::bad_request(format!(
                    "门牌 {} 并非租户「{}」的当前地址",
                    old.label(),
                    tenant.name
                )));
            }
            let target_id = input
                .target_doorplate_id
                .ok_or_else(|| ApiError::bad_request("搬迁申请必须选择目标门牌"))?;
            if target_id == old_id {
                return Err(ApiError::bad_request("目标门牌不能与旧址相同"));
            }
            let target = fetch_doorplate_tx(&mut tx, target_id).await?;
            // 拦截：目标门牌被占用 / 处于转发过渡期
            if let Some(reason) = doorplate_block_reason(&target, tenant.id, &today) {
                return Err(ApiError::conflict(reason));
            }
            // 拦截：目标门牌已被其他进行中的申请占用
            let existing: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM applications WHERE target_doorplate_id = ?
                 AND status IN ('submitted','approved','producing','installed','accepted','paused')",
            )
            .bind(target_id)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some(app_id) = existing {
                return Err(ApiError::conflict(format!(
                    "门牌 {} 已被申请 #{app_id} 占用，不能重复申请",
                    target.label()
                )));
            }
            (None, Some(old_id), Some(target_id))
        }
        "rename" => {
            let name = input
                .new_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| ApiError::bad_request("更名申请必须填写新名称"))?
                .to_string();
            let old_id = input
                .old_doorplate_id
                .ok_or_else(|| ApiError::bad_request("更名申请必须选择门牌"))?;
            let old = fetch_doorplate_tx(&mut tx, old_id).await?;
            if old.tenant_id != Some(tenant.id) {
                return Err(ApiError::bad_request(format!(
                    "门牌 {} 并非租户「{}」的当前地址",
                    old.label(),
                    tenant.name
                )));
            }
            // 更名：门牌不变，目标即现址
            (Some(name), Some(old_id), Some(old_id))
        }
        other => return Err(ApiError::bad_request(format!("不支持的变更类型：{other}"))),
    };

    let now = now();
    let material = input.material.unwrap_or_default();
    let inserted = sqlx::query(
        "INSERT INTO applications
         (tenant_id, change_type, new_name, old_doorplate_id, target_doorplate_id,
          lease_no, lease_start, lease_end, material, effective_date, status, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'submitted', ?, ?)",
    )
    .bind(tenant.id)
    .bind(&input.change_type)
    .bind(&new_name)
    .bind(old_id)
    .bind(target_id)
    .bind(input.lease_no.trim())
    .bind(&input.lease_start)
    .bind(&input.lease_end)
    .bind(&material)
    .bind(&input.effective_date)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await;

    let id = match inserted {
        Ok(res) => res.last_insert_rowid(),
        Err(e) => {
            // 数据库唯一索引兜底：两个申请同时占用同一门牌
            if is_unique_violation(&e) {
                return Err(ApiError::conflict(
                    "该门牌已被其他进行中的申请占用，不能重复申请",
                ));
            }
            return Err(e.into());
        }
    };

    sqlx::query(
        "INSERT INTO state_events (application_id, from_status, to_status, reason, actor, created_at)
         VALUES (?, NULL, 'submitted', '租户在线提交', '租户', ?)",
    )
    .bind(id)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    get_application_view(pool, id).await
}

// ---------- 审核 ----------

pub async fn review(
    pool: &SqlitePool,
    id: i64,
    req: ReviewRequest,
) -> Result<ApplicationView, ApiError> {
    let mut tx = begin_write(pool).await?;
    let app = fetch_application_tx(&mut tx, id).await?;
    if app.status != app_status::SUBMITTED {
        return Err(ApiError::conflict(format!(
            "当前状态为 {}，仅「已提交」的申请可审核",
            app.status
        )));
    }
    let actor = req.actor.as_deref().unwrap_or("物业");
    if req.approve {
        set_status(&mut tx, &app, app_status::APPROVED, Some("审核通过"), actor).await?;
    } else {
        let reason = req
            .reason
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("材料不全，审核驳回");
        set_status(&mut tx, &app, app_status::REJECTED, Some(reason), actor).await?;
    }
    tx.commit().await?;
    get_application_view(pool, id).await
}

// ---------- 制作（调用本地模拟打印接口） ----------

fn build_print_content(app: &Application, target_label: &str, tenant_display: &str) -> String {
    format!(
        "门牌号: {target_label}\n租户: {tenant_display}\n生效日期: {}\n租约编号: {}",
        app.effective_date, app.lease_no
    )
}

/// 本地模拟打印接口的核心：记录一条打印作业。simulate_failure 用于演示「制作有误」。
async fn mock_print_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    application_id: i64,
    content: &str,
    simulate_failure: bool,
) -> Result<PrintJob, ApiError> {
    let (status, message) = if simulate_failure {
        ("failed", Some("模拟打印失败：印刷内容有误"))
    } else {
        ("success", Some("打印完成，待安装"))
    };
    let now = now();
    let res = sqlx::query(
        "INSERT INTO print_jobs (application_id, content, status, message, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(application_id)
    .bind(content)
    .bind(status)
    .bind(message)
    .bind(&now)
    .execute(&mut **tx)
    .await?;
    let job = sqlx::query_as::<_, PrintJob>("SELECT * FROM print_jobs WHERE id = ?")
        .bind(res.last_insert_rowid())
        .fetch_one(&mut **tx)
        .await?;
    Ok(job)
}

pub async fn start_production(
    pool: &SqlitePool,
    id: i64,
    simulate_failure: bool,
) -> Result<ApplicationView, ApiError> {
    let mut tx = begin_write(pool).await?;
    let app = fetch_application_tx(&mut tx, id).await?;
    if app.status != app_status::APPROVED {
        return Err(ApiError::conflict(format!(
            "当前状态为 {}，仅「已审核」的申请可开始制作",
            app.status
        )));
    }

    let tenant = sqlx::query_as::<_, Tenant>("SELECT * FROM tenants WHERE id = ?")
        .bind(app.tenant_id)
        .fetch_one(&mut *tx)
        .await?;
    let display_name = app.new_name.clone().unwrap_or_else(|| tenant.name.clone());
    let target_label = match app.target_doorplate_id {
        Some(tid) => fetch_doorplate_tx(&mut tx, tid).await?.label(),
        None => "-".to_string(),
    };
    let content = build_print_content(&app, &target_label, &display_name);

    set_status(&mut tx, &app, app_status::PRODUCING, Some("开始制作门牌"), "物业").await?;
    let job = mock_print_in_tx(&mut tx, id, &content, simulate_failure).await?;
    if job.status == "failed" {
        // 制作有误 → 暂停发布，提示原因
        let paused_app = Application {
            status: app_status::PRODUCING.to_string(),
            ..app.clone()
        };
        pause_in_tx(
            &mut tx,
            &paused_app,
            &format!("制作有误：{}", job.message.clone().unwrap_or_default()),
        )
        .await?;
    }
    tx.commit().await?;
    get_application_view(pool, id).await
}

// ---------- 安装 / 验收 ----------

pub async fn install(
    pool: &SqlitePool,
    id: i64,
    photo: String,
) -> Result<ApplicationView, ApiError> {
    if photo.trim().is_empty() {
        return Err(ApiError::bad_request("请上传安装照片"));
    }
    let mut tx = begin_write(pool).await?;
    let app = fetch_application_tx(&mut tx, id).await?;
    if app.status != app_status::PRODUCING {
        return Err(ApiError::conflict(format!(
            "当前状态为 {}，仅「制作中」的申请可登记安装",
            app.status
        )));
    }
    sqlx::query("UPDATE applications SET install_photo = ? WHERE id = ?")
        .bind(&photo)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    set_status(&mut tx, &app, app_status::INSTALLED, Some("安装完成，照片已归档"), "物业").await?;
    tx.commit().await?;
    get_application_view(pool, id).await
}

pub async fn accept(
    pool: &SqlitePool,
    id: i64,
    accepted_by: String,
) -> Result<ApplicationView, ApiError> {
    if accepted_by.trim().is_empty() {
        return Err(ApiError::bad_request("请填写验收人"));
    }
    let mut tx = begin_write(pool).await?;
    let app = fetch_application_tx(&mut tx, id).await?;
    if app.status != app_status::INSTALLED {
        return Err(ApiError::conflict(format!(
            "当前状态为 {}，仅「已安装」的申请可验收",
            app.status
        )));
    }
    sqlx::query("UPDATE applications SET accepted_by = ? WHERE id = ?")
        .bind(accepted_by.trim())
        .bind(id)
        .execute(&mut *tx)
        .await?;
    set_status(&mut tx, &app, app_status::ACCEPTED, Some("验收通过"), accepted_by.trim()).await?;
    tx.commit().await?;
    get_application_view(pool, id).await
}

// ---------- 人工暂停 / 恢复 ----------

pub async fn pause(pool: &SqlitePool, id: i64, reason: String) -> Result<ApplicationView, ApiError> {
    if reason.trim().is_empty() {
        return Err(ApiError::bad_request("请填写暂停原因"));
    }
    let mut tx = begin_write(pool).await?;
    let app = fetch_application_tx(&mut tx, id).await?;
    if !app_status::ACTIVE[..5].contains(&app.status.as_str()) {
        return Err(ApiError::conflict(format!(
            "当前状态为 {}，无法暂停",
            app.status
        )));
    }
    pause_in_tx(&mut tx, &app, reason.trim()).await?;
    tx.commit().await?;
    get_application_view(pool, id).await
}

pub async fn resume(pool: &SqlitePool, id: i64) -> Result<ApplicationView, ApiError> {
    let mut tx = begin_write(pool).await?;
    let app = fetch_application_tx(&mut tx, id).await?;
    if app.status != app_status::PAUSED {
        return Err(ApiError::conflict("仅「已暂停」的申请可恢复"));
    }
    let to = app
        .resume_status
        .clone()
        .unwrap_or_else(|| app_status::SUBMITTED.to_string());
    let now = now();
    sqlx::query(
        "UPDATE applications SET status = ?, pause_reason = NULL, resume_status = NULL, updated_at = ?
         WHERE id = ? AND status = 'paused'",
    )
    .bind(&to)
    .bind(&now)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO state_events (application_id, from_status, to_status, reason, actor, created_at)
         VALUES (?, 'paused', ?, '问题已处理，人工恢复', '物业', ?)",
    )
    .bind(id)
    .bind(&to)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    get_application_view(pool, id).await
}

// ---------- 发布（幂等 + 事务） ----------

fn courier_payload(
    app: &Application,
    tenant_display: &str,
    old_label: Option<&str>,
    new_label: Option<&str>,
) -> serde_json::Value {
    let forwarding_until = plus_days(&app.effective_date, 30);
    json!({
        "type": "address_change",
        "tenant": tenant_display,
        "old_address": old_label,
        "new_address": new_label,
        "effective_date": app.effective_date,
        "forwarding_until": forwarding_until,
        "note": format!("{} 起按新址投递；旧址快件在过渡期（至 {}）内转发至新址，旧址不再分配给新租户",
                        app.effective_date, forwarding_until),
    })
}

/// 发布申请：统一切换门牌、导视与快递地址。
/// 幂等：同一申请重复发布只执行一次切换，后续调用返回 already_published。
/// 校验失败（租约未生效 / 目标门牌被占用 / 处于转发期）→ 暂停并记录原因。
pub async fn publish(pool: &SqlitePool, id: i64) -> Result<PublishOutcome, ApiError> {
    let mut tx = begin_write(pool).await?;
    let app = fetch_application_tx(&mut tx, id).await?;

    // 幂等短路：已发布的申请直接返回，不重复任何切换动作
    if app.status == app_status::PUBLISHED {
        tx.commit().await?;
        return Ok(PublishOutcome {
            result: "already_published".into(),
            message: "该申请已发布，重复发布不会再次切换".into(),
            application_id: id,
        });
    }
    if app.status != app_status::ACCEPTED {
        return Err(ApiError::conflict(format!(
            "当前状态为 {}，仅「已验收」的申请可发布",
            app.status
        )));
    }

    // 校验 1：租约已生效（租约起始日不得晚于统一切换日）
    if app.lease_start > app.effective_date {
        let reason = format!(
            "租约未生效：租约起始日 {} 晚于切换日 {}",
            app.lease_start, app.effective_date
        );
        pause_in_tx(&mut tx, &app, &reason).await?;
        tx.commit().await?;
        return Ok(PublishOutcome {
            result: "paused".into(),
            message: format!("发布已暂停：{reason}"),
            application_id: id,
        });
    }

    // 校验 2：目标门牌未被他人占用、不在转发过渡期
    let target = match app.target_doorplate_id {
        Some(tid) => Some(fetch_doorplate_tx(&mut tx, tid).await?),
        None => None,
    };
    if let Some(t) = &target {
        if let Some(reason) = doorplate_block_reason(t, app.tenant_id, &today()) {
            pause_in_tx(&mut tx, &app, &reason).await?;
            tx.commit().await?;
            return Ok(PublishOutcome {
                result: "paused".into(),
                message: format!("发布已暂停：{reason}"),
                application_id: id,
            });
        }
    }

    // 守卫式切换：并发下只有一方能把 accepted -> published
    let now = now();
    let res = sqlx::query(
        "UPDATE applications SET status = 'published', published_at = ?, updated_at = ?
         WHERE id = ? AND status = 'accepted'",
    )
    .bind(&now)
    .bind(&now)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() == 0 {
        // 另一请求已抢先发布 —— 幂等返回，不重复切换
        tx.commit().await?;
        return Ok(PublishOutcome {
            result: "already_published".into(),
            message: "该申请已发布，重复发布不会再次切换".into(),
            application_id: id,
        });
    }

    // ---- 以下全部在同一事务内，要么全部生效要么全部回滚 ----

    let tenant = sqlx::query_as::<_, Tenant>("SELECT * FROM tenants WHERE id = ?")
        .bind(app.tenant_id)
        .fetch_one(&mut *tx)
        .await?;
    let display_name = app.new_name.clone().unwrap_or_else(|| tenant.name.clone());
    let target_label = target.as_ref().map(|t| t.label());

    // 1) 旧址进入 30 天转发过渡期：不再属于任何租户，也不可分配给新租户
    if let Some(old_id) = app.old_doorplate_id {
        if app.change_type == "move" && Some(old_id) != app.target_doorplate_id {
            let until = plus_days(&app.effective_date, 30);
            let to_label = target_label.clone().unwrap_or_default();
            sqlx::query(
                "UPDATE doorplates SET status = 'forwarding', tenant_id = NULL,
                 forwarding_until = ?, forwarding_to = ? WHERE id = ?",
            )
            .bind(&until)
            .bind(&to_label)
            .bind(old_id)
            .execute(&mut *tx)
            .await?;
        }
    }

    // 2) 目标门牌转为占用
    if let Some(tid) = app.target_doorplate_id {
        sqlx::query(
            "UPDATE doorplates SET status = 'occupied', tenant_id = ?,
             forwarding_until = NULL, forwarding_to = NULL WHERE id = ?",
        )
        .bind(app.tenant_id)
        .bind(tid)
        .execute(&mut *tx)
        .await?;
    }

    // 3) 更名：更新租户名称
    if app.change_type == "rename" {
        if let Some(name) = &app.new_name {
            sqlx::query("UPDATE tenants SET name = ? WHERE id = ?")
                .bind(name)
                .bind(app.tenant_id)
                .execute(&mut *tx)
                .await?;
        }
    }

    // 4) 状态事件 + 快递通知（同事务，保证只记录一次）
    sqlx::query(
        "INSERT INTO state_events (application_id, from_status, to_status, reason, actor, created_at)
         VALUES (?, 'accepted', 'published', '统一生效：导视、门牌与快递地址同步切换', '物业', ?)",
    )
    .bind(id)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    let old_label = match app.old_doorplate_id {
        Some(oid) => Some(fetch_doorplate_tx(&mut tx, oid).await?.label()),
        None => None,
    };
    let payload = courier_payload(&app, &display_name, old_label.as_deref(), target_label.as_deref());
    sqlx::query(
        "INSERT INTO courier_notifications (application_id, payload, created_at) VALUES (?, ?, ?)",
    )
    .bind(id)
    .bind(payload.to_string())
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(PublishOutcome {
        result: "published".into(),
        message: "发布成功：门牌、楼内导视与快递地址已同步切换".into(),
        application_id: id,
    })
}

// ---------- 查询：门牌 / 前台导视 / 楼层导视 ----------

pub async fn list_tenants(pool: &SqlitePool) -> Result<Vec<Tenant>, ApiError> {
    Ok(sqlx::query_as::<_, Tenant>("SELECT * FROM tenants ORDER BY id")
        .fetch_all(pool)
        .await?)
}

pub async fn list_doorplates(
    pool: &SqlitePool,
    floor: Option<i64>,
) -> Result<Vec<DoorplateView>, ApiError> {
    let base = r#"
        SELECT d.id, d.floor, d.room, d.status, d.tenant_id, t.name AS tenant_name,
               d.forwarding_until, d.forwarding_to,
               (SELECT a.id FROM applications a
                 WHERE a.target_doorplate_id = d.id
                   AND a.status IN ('submitted','approved','producing','installed','accepted','paused'))
               AS reserved_by_application
        FROM doorplates d
        LEFT JOIN tenants t ON t.id = d.tenant_id
    "#;
    let mut rows: Vec<DoorplateView> = match floor {
        Some(f) => sqlx::query_as::<_, DoorplateView>(&format!("{base} WHERE d.floor = ? ORDER BY d.room"))
            .bind(f)
            .fetch_all(pool)
            .await?,
        None => sqlx::query_as::<_, DoorplateView>(&format!("{base} ORDER BY d.floor, d.room"))
            .fetch_all(pool)
            .await?,
    };
    let today = today();
    for r in &mut rows {
        let dp = Doorplate {
            id: r.id,
            floor: r.floor,
            room: r.room.clone(),
            status: r.status.clone(),
            tenant_id: r.tenant_id,
            forwarding_until: r.forwarding_until.clone(),
            forwarding_to: r.forwarding_to.clone(),
        };
        r.targetable =
            doorplate_block_reason(&dp, -1, &today).is_none() && r.reserved_by_application.is_none();
    }
    Ok(rows)
}

pub async fn directory(pool: &SqlitePool) -> Result<Vec<DirectoryRow>, ApiError> {
    let rows = sqlx::query(
        r#"
        SELECT a.id AS application_id, t.name AS tenant_name, a.new_name, a.change_type,
               (od.floor || 'F ' || od.room) AS old_label,
               (td.floor || 'F ' || td.room) AS new_label,
               a.effective_date, a.status
        FROM applications a
        JOIN tenants t ON t.id = a.tenant_id
        LEFT JOIN doorplates od ON od.id = a.old_doorplate_id
        LEFT JOIN doorplates td ON td.id = a.target_doorplate_id
        WHERE a.status != 'rejected'
        ORDER BY a.effective_date, a.id
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let tenant_name: String = r.get("tenant_name");
            let new_name: Option<String> = r.get("new_name");
            DirectoryRow {
                application_id: r.get("application_id"),
                display_name: new_name.unwrap_or_else(|| tenant_name.clone()),
                tenant_name,
                change_type: r.get("change_type"),
                old_label: r.get("old_label"),
                new_label: r.get("new_label"),
                effective_date: r.get("effective_date"),
                status: r.get("status"),
            }
        })
        .collect())
}

pub async fn wayfinding(pool: &SqlitePool, floor: i64) -> Result<WayfindingFloor, ApiError> {
    let doorplates = sqlx::query(
        r#"
        SELECT d.id, d.room, d.status, t.name AS tenant_name, d.forwarding_until, d.forwarding_to
        FROM doorplates d
        LEFT JOIN tenants t ON t.id = d.tenant_id
        WHERE d.floor = ?
        ORDER BY d.room
        "#,
    )
    .bind(floor)
    .fetch_all(pool)
    .await?;

    // 该楼层相关的进行中申请（作为目标或旧址）
    let active = sqlx::query(
        r#"
        SELECT a.id, a.old_doorplate_id, a.target_doorplate_id, a.effective_date,
               t.name AS tenant_name, a.new_name,
               (od.floor || 'F ' || od.room) AS old_label,
               (td.floor || 'F ' || td.room) AS target_label
        FROM applications a
        JOIN tenants t ON t.id = a.tenant_id
        LEFT JOIN doorplates od ON od.id = a.old_doorplate_id
        LEFT JOIN doorplates td ON td.id = a.target_doorplate_id
        WHERE a.status IN ('submitted','approved','producing','installed','accepted','paused')
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut list = Vec::new();
    for d in &doorplates {
        let did: i64 = d.get("id");
        let mut upcoming: Option<UpcomingChange> = None;
        for a in &active {
            let aid: i64 = a.get("id");
            let target_id: Option<i64> = a.get("target_doorplate_id");
            let old_id: Option<i64> = a.get("old_doorplate_id");
            let tenant_name: String = a.get("tenant_name");
            let new_name: Option<String> = a.get("new_name");
            let display = new_name.unwrap_or_else(|| tenant_name.clone());
            if target_id == Some(did) && old_id != Some(did) {
                upcoming = Some(UpcomingChange {
                    kind: "move_in".into(),
                    application_id: aid,
                    tenant_name: display.clone(),
                    effective_date: a.get("effective_date"),
                    other_label: a.get("old_label"),
                });
            } else if old_id == Some(did) && target_id != Some(did) {
                upcoming = Some(UpcomingChange {
                    kind: "move_out".into(),
                    application_id: aid,
                    tenant_name: display.clone(),
                    effective_date: a.get("effective_date"),
                    other_label: a.get("target_label"),
                });
            }
        }
        list.push(WayfindingDoorplate {
            id: did,
            room: d.get("room"),
            status: d.get("status"),
            tenant_name: d.get("tenant_name"),
            forwarding_until: d.get("forwarding_until"),
            forwarding_to: d.get("forwarding_to"),
            upcoming,
        });
    }
    Ok(WayfindingFloor {
        floor,
        doorplates: list,
    })
}

// ---------- 本地模拟接口（对外暴露） ----------

pub async fn mock_print(
    pool: &SqlitePool,
    req: MockPrintRequest,
) -> Result<PrintJob, ApiError> {
    let mut tx = begin_write(pool).await?;
    let app = fetch_application_tx(&mut tx, req.application_id).await?;
    let content = match req.content {
        Some(c) if !c.trim().is_empty() => c,
        _ => {
            let tenant = sqlx::query_as::<_, Tenant>("SELECT * FROM tenants WHERE id = ?")
                .bind(app.tenant_id)
                .fetch_one(&mut *tx)
                .await?;
            let display = app.new_name.clone().unwrap_or(tenant.name);
            let label = match app.target_doorplate_id {
                Some(tid) => fetch_doorplate_tx(&mut tx, tid).await?.label(),
                None => "-".to_string(),
            };
            build_print_content(&app, &label, &display)
        }
    };
    let job = mock_print_in_tx(&mut tx, app.id, &content, req.simulate_failure).await?;
    // 若申请正在制作中且打印失败 → 暂停并提示原因
    if job.status == "failed" && app.status == app_status::PRODUCING {
        pause_in_tx(
            &mut tx,
            &app,
            &format!("制作有误：{}", job.message.clone().unwrap_or_default()),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(job)
}

pub async fn mock_courier(
    pool: &SqlitePool,
    application_id: i64,
) -> Result<CourierNotification, ApiError> {
    let mut tx = begin_write(pool).await?;
    let app = fetch_application_tx(&mut tx, application_id).await?;
    let tenant = sqlx::query_as::<_, Tenant>("SELECT * FROM tenants WHERE id = ?")
        .bind(app.tenant_id)
        .fetch_one(&mut *tx)
        .await?;
    let display = app.new_name.clone().unwrap_or(tenant.name);
    let old_label = match app.old_doorplate_id {
        Some(oid) => Some(fetch_doorplate_tx(&mut tx, oid).await?.label()),
        None => None,
    };
    let new_label = match app.target_doorplate_id {
        Some(tid) => Some(fetch_doorplate_tx(&mut tx, tid).await?.label()),
        None => None,
    };
    let payload = courier_payload(&app, &display, old_label.as_deref(), new_label.as_deref());
    let now = now();
    let res = sqlx::query(
        "INSERT INTO courier_notifications (application_id, payload, created_at) VALUES (?, ?, ?)",
    )
    .bind(app.id)
    .bind(payload.to_string())
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    let row = sqlx::query_as::<_, CourierNotification>(
        "SELECT * FROM courier_notifications WHERE id = ?",
    )
    .bind(res.last_insert_rowid())
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(row)
}

pub async fn list_print_jobs(pool: &SqlitePool) -> Result<Vec<PrintJob>, ApiError> {
    Ok(
        sqlx::query_as::<_, PrintJob>("SELECT * FROM print_jobs ORDER BY id DESC")
            .fetch_all(pool)
            .await?,
    )
}

pub async fn list_courier_notifications(
    pool: &SqlitePool,
) -> Result<Vec<CourierNotification>, ApiError> {
    Ok(sqlx::query_as::<_, CourierNotification>(
        "SELECT * FROM courier_notifications ORDER BY id DESC",
    )
    .fetch_all(pool)
    .await?)
}
