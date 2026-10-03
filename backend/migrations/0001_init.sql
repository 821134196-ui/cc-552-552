-- 写字楼租户门牌变更与引导更新系统

CREATE TABLE IF NOT EXISTS tenants (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    name             TEXT    NOT NULL,
    current_room_id  INTEGER,
    active           INTEGER NOT NULL DEFAULT 1,
    FOREIGN KEY (current_room_id) REFERENCES rooms(id)
);

CREATE TABLE IF NOT EXISTS rooms (
    id                        INTEGER PRIMARY KEY AUTOINCREMENT,
    floor                     TEXT    NOT NULL,          -- 楼层，如 3F
    code                      TEXT    NOT NULL UNIQUE,   -- 门牌，如 301
    label                     TEXT    NOT NULL DEFAULT '',-- 位置说明，如 电梯口东侧
    status                    TEXT    NOT NULL DEFAULT 'vacant', -- occupied/vacant/reserved/transition
    active_tenant_id          INTEGER,
    incoming_application_id   INTEGER,                   -- 已审核预约占用该门牌的申请
    forward_to_room_id        INTEGER,                   -- 过渡期快件转发目标房间
    forward_until             TEXT,                      -- 转发截止日 yyyy-mm-dd
    forward_tenant_name       TEXT,                      -- 已迁出租户名称（快件转发提示用）
    FOREIGN KEY (active_tenant_id)        REFERENCES tenants(id),
    FOREIGN KEY (incoming_application_id) REFERENCES applications(id),
    FOREIGN KEY (forward_to_room_id)      REFERENCES rooms(id)
);

CREATE TABLE IF NOT EXISTS applications (
    id                     INTEGER PRIMARY KEY AUTOINCREMENT,
    code                   TEXT    NOT NULL UNIQUE,      -- 申请编号 CHG-xxx
    tenant_id              INTEGER,                      -- 既有租户（更名/搬迁）；新租户为空
    tenant_name            TEXT    NOT NULL,             -- 变更后展示名称
    old_room_id            INTEGER,
    new_room_id            INTEGER    NOT NULL,
    lease_ref              TEXT    NOT NULL,             -- 租约材料编号/文件名
    lease_effective_date   TEXT    NOT NULL,             -- 租约生效日
    effective_date         TEXT    NOT NULL,             -- 统一生效/切换日
    status                 TEXT    NOT NULL DEFAULT 'submitted',
    -- submitted -> approved -> fabricated -> installed -> accepted -> published
    -- 任意环节可进入 blocked（附 block_reason / blocked_stage）
    block_reason           TEXT,
    blocked_stage          TEXT,
    fabrication_ok         INTEGER,                       -- 制作是否合格
    fabrication_note       TEXT,
    installer              TEXT,
    install_photo          TEXT,
    installed_at           TEXT,
    accepted_by            TEXT,
    accepted_at            TEXT,
    published_once         INTEGER NOT NULL DEFAULT 0,    -- 幂等闸：是否已完成切换
    published_at           TEXT,
    created_at             TEXT    NOT NULL,
    updated_at             TEXT    NOT NULL,
    FOREIGN KEY (tenant_id)   REFERENCES tenants(id),
    FOREIGN KEY (old_room_id) REFERENCES rooms(id),
    FOREIGN KEY (new_room_id) REFERENCES rooms(id)
);

CREATE TABLE IF NOT EXISTS status_events (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    application_id  INTEGER NOT NULL,
    from_status     TEXT,
    to_status       TEXT    NOT NULL,
    actor           TEXT    NOT NULL DEFAULT '物业',
    note            TEXT,
    created_at      TEXT    NOT NULL,
    FOREIGN KEY (application_id) REFERENCES applications(id)
);

-- 本地模拟接口（打印车间 / 快递网点）的调用记录
CREATE TABLE IF NOT EXISTS notifications (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    application_id  INTEGER,
    kind            TEXT    NOT NULL,   -- print / courier
    target          TEXT    NOT NULL,   -- 接收方
    payload         TEXT    NOT NULL,
    result          TEXT    NOT NULL,
    created_at      TEXT    NOT NULL,
    FOREIGN KEY (application_id) REFERENCES applications(id)
);

CREATE INDEX IF NOT EXISTS idx_applications_status ON applications(status);
CREATE INDEX IF NOT EXISTS idx_events_app ON status_events(application_id);

-- 硬约束：同一门牌在任一时刻只能被一个“进行中”的申请占用
-- （approved/fabricated/installed/accepted）。published/blocked/submitted 不占位。
CREATE UNIQUE INDEX IF NOT EXISTS uq_application_active_target
ON applications(new_room_id)
WHERE status IN ('approved', 'fabricated', 'installed', 'accepted');
