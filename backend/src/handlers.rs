use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::error::AppError;
use crate::service::{
    accept, approve, create_application, fabricate, get_application_detail, guide, install,
    list_applications, list_notifications, list_rooms, list_tenants, publish, resubmit,
    AcceptInput, CreateInput, FabricateInput, InstallInput, PatchInput,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
}

pub fn router(pool: SqlitePool) -> Router {
    let state = AppState { pool };
    Router::new()
        .route("/api/health", get(|| async { Json(json!({ "ok": true })) }))
        .route("/api/rooms", get(get_rooms))
        .route("/api/tenants", get(get_tenants))
        .route("/api/guide", get(get_guide))
        .route("/api/applications", get(list_apps).post(post_application))
        .route(
            "/api/applications/{id}",
            get(get_application).patch(patch_application),
        )
        .route("/api/applications/{id}/approve", post(do_approve))
        .route("/api/applications/{id}/fabricate", post(do_fabricate))
        .route("/api/applications/{id}/install", post(do_install))
        .route("/api/applications/{id}/accept", post(do_accept))
        .route("/api/applications/{id}/publish", post(do_publish))
        .route("/api/notifications", get(get_notifications))
        .with_state(state)
}

async fn get_rooms(State(s): State<AppState>) -> Result<Json<Value>, AppError> {
    Ok(Json(json!({ "rooms": list_rooms(&s.pool).await? })))
}

async fn get_tenants(State(s): State<AppState>) -> Result<Json<Value>, AppError> {
    Ok(Json(json!({ "tenants": list_tenants(&s.pool).await? })))
}

async fn get_guide(State(s): State<AppState>) -> Result<Json<Value>, AppError> {
    Ok(Json(guide(&s.pool).await?))
}

async fn list_apps(State(s): State<AppState>) -> Result<Json<Value>, AppError> {
    Ok(Json(json!({ "applications": list_applications(&s.pool).await? })))
}

async fn post_application(
    State(s): State<AppState>,
    Json(input): Json<CreateInput>,
) -> Result<Json<Value>, AppError> {
    let detail = create_application(&s.pool, input).await?;
    Ok(Json(json!({ "application": detail })))
}

async fn get_application(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(json!({ "application": get_application_detail(&s.pool, id).await? })))
}

async fn patch_application(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<PatchInput>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(json!({ "application": resubmit(&s.pool, id, input).await? })))
}

async fn do_approve(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, AppError> {
    Ok(Json(json!({ "application": approve(&s.pool, id).await? })))
}

async fn do_fabricate(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<FabricateInput>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(json!({ "application": fabricate(&s.pool, id, input).await? })))
}

async fn do_install(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<InstallInput>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(json!({ "application": install(&s.pool, id, input).await? })))
}

async fn do_accept(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<AcceptInput>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(json!({ "application": accept(&s.pool, id, input).await? })))
}

async fn do_publish(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, AppError> {
    let outcome = publish(&s.pool, id).await?;
    Ok(Json(json!({
        "switched": outcome.switched,
        "idempotent": !outcome.switched,
        "application": outcome.detail,
    })))
}

async fn get_notifications(State(s): State<AppState>) -> Result<Json<Value>, AppError> {
    Ok(Json(json!({ "notifications": list_notifications(&s.pool).await? })))
}
