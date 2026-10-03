//! 端到端 API 测试：内存 SQLite + axum oneshot。
//! 覆盖需求要求的两个演示场景：
//!   1. 门牌占用被拦截（含两个申请同时占用同一门牌）
//!   2. 重复发布同一申请只执行一次切换（幂等）
//! 以及：租约未生效暂停、转发期门牌不可再分配、并发发布幂等。

use axum::body::Body;
use axum::http::{Request, StatusCode};
use doorplate_server::models::ReviewRequest;
use doorplate_server::{build_router, db, service};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn test_app() -> axum::Router {
    let pool = db::init_pool("sqlite::memory:").await.unwrap();
    db::seed_if_empty(&pool).await.unwrap();
    build_router(pool)
}

async fn call(app: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.map(|b| b.to_string()).unwrap_or_default()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let v = if bytes.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({"_raw": String::from_utf8_lossy(&bytes)}))
    };
    (status, v)
}

/// 把申请从「已提交」推进到「已验收」。
async fn drive_to_accepted(app: &axum::Router, id: i64) {
    let (s, b) = call(app, "POST", &format!("/api/applications/{id}/review"), Some(json!({"approve": true}))).await;
    assert_eq!(s, StatusCode::OK, "review failed: {b}");
    let (s, b) = call(app, "POST", &format!("/api/applications/{id}/produce"), Some(json!({}))).await;
    assert_eq!(s, StatusCode::OK, "produce failed: {b}");
    let (s, b) = call(app, "POST", &format!("/api/applications/{id}/install"), Some(json!({"photo": "install.jpg"}))).await;
    assert_eq!(s, StatusCode::OK, "install failed: {b}");
    let (s, b) = call(app, "POST", &format!("/api/applications/{id}/accept"), Some(json!({"accepted_by": "王验收"}))).await;
    assert_eq!(s, StatusCode::OK, "accept failed: {b}");
}

fn move_payload(tenant: i64, old: i64, target: i64, lease_start: &str, effective: &str) -> Value {
    json!({
        "tenant_id": tenant, "change_type": "move",
        "old_doorplate_id": old, "target_doorplate_id": target,
        "lease_no": "ZL-TEST", "lease_start": lease_start,
        "material": "lease.pdf", "effective_date": effective
    })
}

// 种子数据门牌 ID：1=101(星辰), 2=102, 3=103(蓝海), 4=201(云图), 5=202, 6=203,
//                  7=301, 8=302(绿洲), 9=303(恒信), 10=305
// 种子申请 #1：星辰科技 101 -> 202，已提交。

#[tokio::test]
async fn occupied_target_is_rejected() {
    let app = test_app().await;
    // 蓝海咨询（现址 103）申请搬到 101 —— 101 被星辰科技占用
    let (status, body) = call(&app, "POST", "/api/applications", Some(move_payload(2, 3, 1, "2026-01-01", "2099-01-01"))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["message"].as_str().unwrap().contains("已被占用"), "unexpected: {body}");
}

#[tokio::test]
async fn double_booking_same_doorplate_is_rejected() {
    let app = test_app().await;
    // 两个租户先后申请同一空闲门牌 203（id=6）
    let (s1, _) = call(&app, "POST", "/api/applications", Some(move_payload(1, 1, 6, "2026-01-01", "2099-01-01"))).await;
    assert_eq!(s1, StatusCode::CREATED);
    let (s2, body) = call(&app, "POST", "/api/applications", Some(move_payload(2, 3, 6, "2026-01-01", "2099-01-01"))).await;
    assert_eq!(s2, StatusCode::CONFLICT);
    assert!(body["message"].as_str().unwrap().contains("占用"), "unexpected: {body}");
}

#[tokio::test]
async fn duplicate_publish_executes_only_once() {
    let app = test_app().await;
    drive_to_accepted(&app, 1).await;

    let (s1, b1) = call(&app, "POST", "/api/applications/1/publish", None).await;
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(b1["result"], "published");

    // 重复发布：返回 already_published，不产生二次切换
    let (s2, b2) = call(&app, "POST", "/api/applications/1/publish", None).await;
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(b2["result"], "already_published");

    let (_, detail) = call(&app, "GET", "/api/applications/1", None).await;
    let published_events = detail["events"].as_array().unwrap().iter()
        .filter(|e| e["to_status"] == "published").count();
    assert_eq!(published_events, 1, "published 事件只能有一次");
    assert_eq!(detail["courier_notifications"].as_array().unwrap().len(), 1, "快递通知只能发一次");

    // 门牌已切换：202 归星辰科技，101 进入转发过渡期
    let (_, plates) = call(&app, "GET", "/api/doorplates", None).await;
    let arr = plates.as_array().unwrap();
    let p202 = arr.iter().find(|p| p["room"] == "202").unwrap();
    assert_eq!(p202["status"], "occupied");
    assert_eq!(p202["tenant_id"], 1);
    let p101 = arr.iter().find(|p| p["room"] == "101").unwrap();
    assert_eq!(p101["status"], "forwarding");
    assert!(p101["tenant_id"].is_null());
    assert_eq!(p101["forwarding_to"], "2F 202");
}

#[tokio::test]
async fn publish_pauses_when_lease_not_effective() {
    let app = test_app().await;
    // 租约起始日晚于切换日 → 发布时应暂停
    let (s, created) = call(&app, "POST", "/api/applications", Some(move_payload(2, 3, 6, "2099-06-01", "2099-01-01"))).await;
    assert_eq!(s, StatusCode::CREATED);
    let id = created["id"].as_i64().unwrap();
    drive_to_accepted(&app, id).await;

    let (s, body) = call(&app, "POST", &format!("/api/applications/{id}/publish"), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["result"], "paused");
    assert!(body["message"].as_str().unwrap().contains("租约未生效"), "unexpected: {body}");

    let (_, detail) = call(&app, "GET", &format!("/api/applications/{id}"), None).await;
    assert_eq!(detail["application"]["status"], "paused");
    assert!(detail["application"]["pause_reason"].as_str().unwrap().contains("租约未生效"));
}

#[tokio::test]
async fn forwarding_doorplate_cannot_be_assigned_to_new_tenant() {
    let app = test_app().await;
    // 蓝海咨询 103 -> 203，发布完成后 103 进入转发过渡期
    let (_, created) = call(&app, "POST", "/api/applications", Some(move_payload(2, 3, 6, "2026-01-01", "2099-01-01"))).await;
    let id = created["id"].as_i64().unwrap();
    drive_to_accepted(&app, id).await;
    let (_, b) = call(&app, "POST", &format!("/api/applications/{id}/publish"), None).await;
    assert_eq!(b["result"], "published");

    // 星辰科技申请搬到 103 —— 转发期内不可再分配
    let (s, body) = call(&app, "POST", "/api/applications", Some(move_payload(1, 1, 3, "2026-01-01", "2099-02-01"))).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert!(body["message"].as_str().unwrap().contains("转发过渡期"), "unexpected: {body}");
}

#[tokio::test]
async fn production_failure_pauses_application() {
    let app = test_app().await;
    let (s, _) = call(&app, "POST", "/api/applications/1/review", Some(json!({"approve": true}))).await;
    assert_eq!(s, StatusCode::OK);
    // 模拟打印失败 → 制作有误 → 暂停
    let (s, b) = call(&app, "POST", "/api/applications/1/produce", Some(json!({"simulate_failure": true}))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["status"], "paused");
    assert!(b["pause_reason"].as_str().unwrap().contains("制作有误"), "unexpected: {b}");
}

#[tokio::test]
async fn concurrent_publish_is_idempotent() {
    // 用文件库（多连接）真实模拟两个并发发布请求
    let unique = format!(
        "doorplate-test-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    );
    let path = std::env::temp_dir().join(unique);
    let url = format!("sqlite:{}", path.display());
    let pool = db::init_pool(&url).await.unwrap();
    db::seed_if_empty(&pool).await.unwrap();

    service::review(&pool, 1, ReviewRequest { approve: true, reason: None, actor: None }).await.unwrap();
    service::start_production(&pool, 1, false).await.unwrap();
    service::install(&pool, 1, "p.jpg".into()).await.unwrap();
    service::accept(&pool, 1, "验收员".into()).await.unwrap();

    let (r1, r2) = tokio::join!(
        service::publish(&pool, 1),
        service::publish(&pool, 1)
    );
    let mut results: Vec<String> = vec![r1.unwrap().result, r2.unwrap().result];
    results.sort();
    assert_eq!(results, vec!["already_published".to_string(), "published".to_string()],
        "并发发布应恰好一次生效、一次幂等返回");

    let notes = service::list_courier_notifications(&pool).await.unwrap();
    assert_eq!(notes.len(), 1, "快递通知只能发一次");

    let detail_pool = pool.clone();
    let view = service::get_application_view(&detail_pool, 1).await.unwrap();
    assert_eq!(view.status, "published");

    let _ = std::fs::remove_file(&path);
}
