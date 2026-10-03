use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Application {
    pub id: i64,
    pub code: String,
    pub tenant_id: Option<i64>,
    pub tenant_name: String,
    pub old_room_id: Option<i64>,
    pub new_room_id: i64,
    pub lease_ref: String,
    pub lease_effective_date: String,
    pub effective_date: String,
    pub status: String,
    pub block_reason: Option<String>,
    pub blocked_stage: Option<String>,
    pub fabrication_ok: Option<i64>,
    pub fabrication_note: Option<String>,
    pub installer: Option<String>,
    pub install_photo: Option<String>,
    pub installed_at: Option<String>,
    pub accepted_by: Option<String>,
    pub accepted_at: Option<String>,
    pub published_once: i64,
    pub published_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Room {
    pub id: i64,
    pub floor: String,
    pub code: String,
    pub label: String,
    pub status: String,
    pub active_tenant_id: Option<i64>,
    pub incoming_application_id: Option<i64>,
    pub forward_to_room_id: Option<i64>,
    pub forward_until: Option<String>,
    pub forward_tenant_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Tenant {
    pub id: i64,
    pub name: String,
    pub current_room_id: Option<i64>,
    pub active: i64,
}

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct StatusEvent {
    pub id: i64,
    pub application_id: i64,
    pub from_status: Option<String>,
    pub to_status: String,
    pub actor: String,
    pub note: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub id: i64,
    pub application_id: Option<i64>,
    pub kind: String,
    pub target: String,
    pub payload: String,
    pub result: String,
    pub created_at: String,
}

/// 房间在导视中的状态
pub const ROOM_OCCUPIED: &str = "occupied";
pub const ROOM_VACANT: &str = "vacant";
pub const ROOM_RESERVED: &str = "reserved";
pub const ROOM_TRANSITION: &str = "transition";
