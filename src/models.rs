use serde::{Deserialize, Serialize};
use sqlx::FromRow;

// ---------- 状态常量 ----------

pub mod app_status {
    pub const SUBMITTED: &str = "submitted";
    pub const APPROVED: &str = "approved";
    pub const REJECTED: &str = "rejected";
    pub const PRODUCING: &str = "producing";
    pub const INSTALLED: &str = "installed";
    pub const ACCEPTED: &str = "accepted";
    pub const PAUSED: &str = "paused";
    pub const PUBLISHED: &str = "published";

    /// 进行中（未终结）的状态，占用目标门牌
    pub const ACTIVE: [&str; 6] = [SUBMITTED, APPROVED, PRODUCING, INSTALLED, ACCEPTED, PAUSED];
}

pub mod door_status {
    pub const AVAILABLE: &str = "available";
    pub const OCCUPIED: &str = "occupied";
    pub const FORWARDING: &str = "forwarding";
}

// ---------- 表模型 ----------

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Tenant {
    pub id: i64,
    pub name: String,
    pub contact: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Doorplate {
    pub id: i64,
    pub floor: i64,
    pub room: String,
    pub status: String,
    pub tenant_id: Option<i64>,
    pub forwarding_until: Option<String>,
    pub forwarding_to: Option<String>,
}

impl Doorplate {
    pub fn label(&self) -> String {
        format!("{}F {}", self.floor, self.room)
    }
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Application {
    pub id: i64,
    pub tenant_id: i64,
    pub change_type: String,
    pub new_name: Option<String>,
    pub old_doorplate_id: Option<i64>,
    pub target_doorplate_id: Option<i64>,
    pub lease_no: String,
    pub lease_start: String,
    pub lease_end: Option<String>,
    pub material: String,
    pub effective_date: String,
    pub status: String,
    pub pause_reason: Option<String>,
    pub resume_status: Option<String>,
    pub install_photo: Option<String>,
    pub accepted_by: Option<String>,
    pub published_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct StateEvent {
    pub id: i64,
    pub application_id: i64,
    pub from_status: Option<String>,
    pub to_status: String,
    pub reason: Option<String>,
    pub actor: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct PrintJob {
    pub id: i64,
    pub application_id: i64,
    pub content: String,
    pub status: String,
    pub message: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct CourierNotification {
    pub id: i64,
    pub application_id: i64,
    pub payload: String,
    pub created_at: String,
}

// ---------- 视图模型（JOIN 查询） ----------

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ApplicationView {
    pub id: i64,
    pub tenant_id: i64,
    pub tenant_name: String,
    pub change_type: String,
    pub new_name: Option<String>,
    pub old_doorplate_id: Option<i64>,
    pub old_label: Option<String>,
    pub target_doorplate_id: Option<i64>,
    pub target_label: Option<String>,
    pub lease_no: String,
    pub lease_start: String,
    pub lease_end: Option<String>,
    pub material: String,
    pub effective_date: String,
    pub status: String,
    pub pause_reason: Option<String>,
    pub install_photo: Option<String>,
    pub accepted_by: Option<String>,
    pub published_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct DoorplateView {
    pub id: i64,
    pub floor: i64,
    pub room: String,
    pub status: String,
    pub tenant_id: Option<i64>,
    pub tenant_name: Option<String>,
    pub forwarding_until: Option<String>,
    pub forwarding_to: Option<String>,
    /// 占用该门牌的进行中申请（若有）
    pub reserved_by_application: Option<i64>,
    /// 当前是否可被选为目标门牌（空闲，或转发期已结束，且未被进行中申请占用）— 由后端计算
    #[sqlx(skip)]
    pub targetable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplicationDetail {
    pub application: ApplicationView,
    pub events: Vec<StateEvent>,
    pub print_jobs: Vec<PrintJob>,
    pub courier_notifications: Vec<CourierNotification>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DirectoryRow {
    pub application_id: i64,
    pub tenant_name: String,
    pub display_name: String,
    pub change_type: String,
    pub old_label: Option<String>,
    pub new_label: Option<String>,
    pub effective_date: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpcomingChange {
    pub kind: String, // move_in 即将入驻 / move_out 即将迁出
    pub application_id: i64,
    pub tenant_name: String,
    pub effective_date: String,
    pub other_label: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WayfindingDoorplate {
    pub id: i64,
    pub room: String,
    pub status: String,
    pub tenant_name: Option<String>,
    pub forwarding_until: Option<String>,
    pub forwarding_to: Option<String>,
    pub upcoming: Option<UpcomingChange>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WayfindingFloor {
    pub floor: i64,
    pub doorplates: Vec<WayfindingDoorplate>,
}

// ---------- 请求体 ----------

#[derive(Debug, Deserialize)]
pub struct SubmitApplication {
    pub tenant_id: i64,
    pub change_type: String, // move / rename
    pub new_name: Option<String>,
    pub old_doorplate_id: Option<i64>,
    pub target_doorplate_id: Option<i64>,
    pub lease_no: String,
    pub lease_start: String,
    pub lease_end: Option<String>,
    pub material: Option<String>,
    pub effective_date: String,
}

#[derive(Debug, Deserialize)]
pub struct ReviewRequest {
    pub approve: bool,
    pub reason: Option<String>,
    pub actor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ProduceRequest {
    #[serde(default)]
    pub simulate_failure: bool,
}

#[derive(Debug, Deserialize)]
pub struct InstallRequest {
    pub photo: String,
}

#[derive(Debug, Deserialize)]
pub struct AcceptRequest {
    pub accepted_by: String,
}

#[derive(Debug, Deserialize)]
pub struct PauseRequest {
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct MockPrintRequest {
    pub application_id: i64,
    pub content: Option<String>,
    #[serde(default)]
    pub simulate_failure: bool,
}

#[derive(Debug, Deserialize)]
pub struct MockCourierRequest {
    pub application_id: i64,
}

// ---------- 发布结果 ----------

#[derive(Debug, Clone, Serialize)]
pub struct PublishOutcome {
    /// published 首次发布成功 / already_published 重复发布被幂等忽略 / paused 校验失败已暂停
    pub result: String,
    pub message: String,
    pub application_id: i64,
}
