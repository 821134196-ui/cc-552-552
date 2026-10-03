use crate::error::ApiError;
use crate::models::*;
use crate::service;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::sqlite::SqlitePool;
use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
}

#[derive(Deserialize)]
struct ListQuery {
    status: Option<String>,
}

#[derive(Deserialize)]
struct FloorQuery {
    floor: Option<i64>,
}

pub fn router(pool: SqlitePool) -> Router {
    let state = AppState { pool };
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "frontend/dist".to_string());

    Router::new()
        .route("/api/health", get(health))
        .route("/api/tenants", get(list_tenants))
        .route("/api/doorplates", get(list_doorplates))
        .route("/api/applications", get(list_applications).post(submit_application))
        .route("/api/applications/{id}", get(get_application))
        .route("/api/applications/{id}/review", post(review))
        .route("/api/applications/{id}/produce", post(produce))
        .route("/api/applications/{id}/install", post(install))
        .route("/api/applications/{id}/accept", post(accept))
        .route("/api/applications/{id}/publish", post(publish))
        .route("/api/applications/{id}/pause", post(pause))
        .route("/api/applications/{id}/resume", post(resume))
        .route("/api/directory", get(directory))
        .route("/api/floors/{floor}/wayfinding", get(wayfinding))
        // 本地模拟接口：打印 / 快递通知
        .route("/api/mock/print", post(mock_print))
        .route("/api/mock/courier", post(mock_courier))
        .route("/api/mock/print-jobs", get(list_print_jobs))
        .route("/api/mock/courier-notifications", get(list_courier_notifications))
        .with_state(state)
        .fallback_service(
            ServeDir::new(static_dir.clone())
                .not_found_service(ServeFile::new(format!("{static_dir}/index.html"))),
        )
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "ok": true }))
}

async fn list_tenants(State(s): State<AppState>) -> Result<Json<Vec<Tenant>>, ApiError> {
    Ok(Json(service::list_tenants(&s.pool).await?))
}

async fn list_doorplates(
    State(s): State<AppState>,
    Query(q): Query<FloorQuery>,
) -> Result<Json<Vec<DoorplateView>>, ApiError> {
    Ok(Json(service::list_doorplates(&s.pool, q.floor).await?))
}

async fn list_applications(
    State(s): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<ApplicationView>>, ApiError> {
    Ok(Json(service::list_applications(&s.pool, q.status).await?))
}

async fn submit_application(
    State(s): State<AppState>,
    Json(input): Json<SubmitApplication>,
) -> Result<(StatusCode, Json<ApplicationView>), ApiError> {
    let view = service::submit_application(&s.pool, input).await?;
    Ok((StatusCode::CREATED, Json(view)))
}

async fn get_application(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ApplicationDetail>, ApiError> {
    Ok(Json(service::get_detail(&s.pool, id).await?))
}

async fn review(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<ReviewRequest>,
) -> Result<Json<ApplicationView>, ApiError> {
    Ok(Json(service::review(&s.pool, id, req).await?))
}

async fn produce(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<ProduceRequest>,
) -> Result<Json<ApplicationView>, ApiError> {
    Ok(Json(service::start_production(&s.pool, id, req.simulate_failure).await?))
}

async fn install(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<InstallRequest>,
) -> Result<Json<ApplicationView>, ApiError> {
    Ok(Json(service::install(&s.pool, id, req.photo).await?))
}

async fn accept(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<AcceptRequest>,
) -> Result<Json<ApplicationView>, ApiError> {
    Ok(Json(service::accept(&s.pool, id, req.accepted_by).await?))
}

async fn publish(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<PublishOutcome>, ApiError> {
    Ok(Json(service::publish(&s.pool, id).await?))
}

async fn pause(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<PauseRequest>,
) -> Result<Json<ApplicationView>, ApiError> {
    Ok(Json(service::pause(&s.pool, id, req.reason).await?))
}

async fn resume(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ApplicationView>, ApiError> {
    Ok(Json(service::resume(&s.pool, id).await?))
}

async fn directory(State(s): State<AppState>) -> Result<Json<Vec<DirectoryRow>>, ApiError> {
    Ok(Json(service::directory(&s.pool).await?))
}

async fn wayfinding(
    State(s): State<AppState>,
    Path(floor): Path<i64>,
) -> Result<Json<WayfindingFloor>, ApiError> {
    Ok(Json(service::wayfinding(&s.pool, floor).await?))
}

async fn mock_print(
    State(s): State<AppState>,
    Json(req): Json<MockPrintRequest>,
) -> Result<Json<PrintJob>, ApiError> {
    Ok(Json(service::mock_print(&s.pool, req).await?))
}

async fn mock_courier(
    State(s): State<AppState>,
    Json(req): Json<MockCourierRequest>,
) -> Result<Json<CourierNotification>, ApiError> {
    Ok(Json(service::mock_courier(&s.pool, req.application_id).await?))
}

async fn list_print_jobs(State(s): State<AppState>) -> Result<Json<Vec<PrintJob>>, ApiError> {
    Ok(Json(service::list_print_jobs(&s.pool).await?))
}

async fn list_courier_notifications(
    State(s): State<AppState>,
) -> Result<Json<Vec<CourierNotification>>, ApiError> {
    Ok(Json(service::list_courier_notifications(&s.pool).await?))
}
