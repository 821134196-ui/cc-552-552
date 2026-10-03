-- 写字楼租户门牌变更与引导更新系统 — 初始 schema

CREATE TABLE IF NOT EXISTS tenants (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    name     TEXT NOT NULL,
    contact  TEXT NOT NULL DEFAULT ''
);

-- 门牌（房间单元）。status: available 空闲 / occupied 占用中 / forwarding 旧址转发过渡期
CREATE TABLE IF NOT EXISTS doorplates (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    floor            INTEGER NOT NULL,
    room             TEXT NOT NULL,
    status           TEXT NOT NULL DEFAULT 'available',
    tenant_id        INTEGER REFERENCES tenants(id),
    forwarding_until TEXT,          -- 过渡期截止日期（含），此前旧址快件转发至新址
    forwarding_to    TEXT,          -- 转发目标（如 "2F 202"）
    UNIQUE(floor, room)
);

-- 变更申请。status 状态机:
--   submitted 已提交 -> approved 已审核 -> producing 制作中 -> installed 已安装
--   -> accepted 已验收 -> published 已发布
--   任意进行中状态 -> paused 已暂停（pause_reason 记录原因，resume_status 记录恢复目标）
--   submitted/approved -> rejected 已驳回
CREATE TABLE IF NOT EXISTS applications (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    tenant_id          INTEGER NOT NULL REFERENCES tenants(id),
    change_type        TEXT NOT NULL,               -- move 搬迁 / rename 更名
    new_name           TEXT,                        -- 更名后的新名称
    old_doorplate_id   INTEGER REFERENCES doorplates(id),
    target_doorplate_id INTEGER REFERENCES doorplates(id),
    lease_no           TEXT NOT NULL,               -- 租约编号
    lease_start        TEXT NOT NULL,               -- 租约生效日 YYYY-MM-DD
    lease_end          TEXT,
    material           TEXT NOT NULL DEFAULT '',    -- 租约材料（文件名）
    effective_date     TEXT NOT NULL,               -- 统一切换生效日
    status             TEXT NOT NULL DEFAULT 'submitted',
    pause_reason       TEXT,
    resume_status      TEXT,
    install_photo      TEXT,                        -- 安装照片（data URL 或文件名）
    accepted_by        TEXT,                        -- 验收人
    published_at       TEXT,
    created_at         TEXT NOT NULL,
    updated_at         TEXT NOT NULL
);

-- 防止两个进行中的申请同时占用同一目标门牌（数据库层兜底，配合事务内预检）
CREATE UNIQUE INDEX IF NOT EXISTS idx_applications_active_target
    ON applications(target_doorplate_id)
    WHERE target_doorplate_id IS NOT NULL
      AND status IN ('submitted','approved','producing','installed','accepted','paused');

-- 状态变化流水（审计）
CREATE TABLE IF NOT EXISTS state_events (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    application_id INTEGER NOT NULL REFERENCES applications(id),
    from_status    TEXT,
    to_status      TEXT NOT NULL,
    reason         TEXT,
    actor          TEXT NOT NULL DEFAULT '物业',
    created_at     TEXT NOT NULL
);

-- 本地模拟打印接口的作业记录
CREATE TABLE IF NOT EXISTS print_jobs (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    application_id INTEGER NOT NULL REFERENCES applications(id),
    content        TEXT NOT NULL,
    status         TEXT NOT NULL,                   -- success / failed
    message        TEXT,
    created_at     TEXT NOT NULL
);

-- 本地模拟快递通知接口的记录（发布后通知快递系统更新地址）
CREATE TABLE IF NOT EXISTS courier_notifications (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    application_id INTEGER NOT NULL REFERENCES applications(id),
    payload        TEXT NOT NULL,                   -- JSON
    created_at     TEXT NOT NULL
);
