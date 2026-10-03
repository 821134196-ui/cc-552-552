use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    NotFound(String),
    Conflict(String),
    Internal(String),
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        // SQLite 唯一约束冲突（如两个申请同时占用同一门牌）映射为 409
        if let sqlx::Error::Database(db_err) = &e {
            if db_err.is_unique_violation() {
                return Self::Conflict(format!("唯一约束冲突：{db_err}"));
            }
        }
        Self::Internal(format!("数据库错误：{e}"))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, kind, message) = match self {
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, "bad_request", m),
            Self::NotFound(m) => (StatusCode::NOT_FOUND, "not_found", m),
            Self::Conflict(m) => (StatusCode::CONFLICT, "conflict", m),
            Self::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, "internal", m),
        };
        (status, Json(json!({ "error": kind, "message": message }))).into_response()
    }
}
