# 写字楼租户门牌变更与引导更新系统

租户搬迁或更名后，物业在一个系统里统一更新**楼内导视、实体门牌和快递地址**，
让访客与快递员在切换当天找到正确位置。

## 功能

- **申请与审核流**：租户提交新地址、新名称与租约材料 → 物业审核 → 门牌制作（模拟打印接口）→ 安装（照片归档）→ 验收（验收人记录）→ 发布。
- **统一生效日**：导视、门牌、快递地址使用同一个 `effective_date` 切换。
- **前台导视**：同时展示旧址、新址与切换日期；楼层导视标出「即将入驻 / 即将迁出」。
- **暂停与提示**：目标门牌已被占用、租约未生效、制作有误时自动暂停并记录原因，处理后可恢复。
- **旧址过渡期**：发布后旧址进入 30 天转发期（快件转发至新址），期间不可再分配给新租户。
- **幂等与互斥**：重复发布同一申请只执行一次切换；两个申请不能同时占用同一门牌（事务预检 + 数据库部分唯一索引双保险）。

## 技术栈

| 层 | 技术 |
|---|---|
| 后端 | Rust + Axum 0.8 |
| 前端 | Svelte 4 + Vite（构建后由后端托管） |
| 数据库 | SQLite（SQLx，启动时自动迁移） |
| 外部接口 | 本地模拟：打印接口 `/api/mock/print`、快递通知 `/api/mock/courier` |

## 一键启动

```bash
./start.sh
```

首次运行会安装前端依赖、构建前端、编译后端，然后启动服务。
打开 **http://localhost:3000** 即可使用（首次启动自动建表并写入演示数据）。

环境变量（可选）：`DATABASE_URL`（默认 `sqlite:doorplate.db`）、`BIND_ADDR`（默认 `0.0.0.0:3000`）、`STATIC_DIR`（默认 `frontend/dist`）。

## 演示（需求要求的两个场景）

服务启动后另开终端执行：

```bash
./demo.sh
```

脚本会演示并断言：

1. **门牌占用被拦截** — 申请搬到已被占用的门牌返回 `409`；两个申请抢占同一空闲门牌时第二个返回 `409`。
2. **重复发布只执行一次** — 同一申请连续发布两次：第一次 `published`，第二次 `already_published`；状态流水里 `published` 事件只有一条，快递通知只有一条，门牌只切换一次。

也可以在页面上手动演示：申请详情页中，已发布的申请仍可点击「再次发布」，系统会幂等返回而不二次切换。

## 测试

```bash
cargo test
```

集成测试（`tests/api.rs`，内存 SQLite）覆盖：占用拦截、双申请互斥、重复发布幂等、
租约未生效暂停、转发期门牌不可分配、制作失败暂停、并发发布幂等。

## 状态机

```
submitted ──审核──> approved ──制作──> producing ──安装──> installed ──验收──> accepted ──发布──> published
    │                  │                  │                 │                │
    └──驳回──> rejected└──────────────────┴──── 暂停（pause）┴────────────────┘
                                              │  记录原因 + 恢复点
                                              └──恢复──> 回到原状态
```

发布时的校验（任一失败 → 自动暂停并写明原因）：

- 租约未生效（`lease_start > effective_date`）
- 目标门牌已被他人占用
- 目标门牌处于旧址转发过渡期

发布成功在同一事务内完成：旧址转 `forwarding`（30 天）、目标门牌转 `occupied`、
更名生效、写入状态事件、发送快递通知。重复发布通过「守卫式状态更新 +
已发布短路」保证只执行一次。

## API 一览

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/api/tenants` | 租户列表 |
| GET | `/api/doorplates?floor=` | 门牌列表（含状态、可分配标记） |
| POST | `/api/applications` | 提交申请（占用/转发期/重复占用 → 409） |
| GET | `/api/applications?status=` | 申请列表 |
| GET | `/api/applications/{id}` | 申请详情（含事件、打印、快递记录） |
| POST | `/api/applications/{id}/review` | 审核 `{approve, reason?}` |
| POST | `/api/applications/{id}/produce` | 开始制作 `{simulate_failure?}` |
| POST | `/api/applications/{id}/install` | 登记安装 `{photo}` |
| POST | `/api/applications/{id}/accept` | 验收 `{accepted_by}` |
| POST | `/api/applications/{id}/publish` | 发布（幂等） |
| POST | `/api/applications/{id}/pause` `/resume` | 暂停 / 恢复 |
| GET | `/api/directory` | 前台导视（旧址/新址/切换日） |
| GET | `/api/floors/{floor}/wayfinding` | 楼层导视（含即将变更） |
| POST | `/api/mock/print` | 模拟打印接口 |
| POST | `/api/mock/courier` | 模拟快递通知接口 |
| GET | `/api/mock/print-jobs` `/api/mock/courier-notifications` | 模拟接口记录 |

## 目录结构

```
├── Cargo.toml            # 后端 crate
├── migrations/           # SQLx 迁移（建表 + 部分唯一索引）
├── src/
│   ├── main.rs           # 入口：连库、迁移、种子数据、起服务
│   ├── lib.rs
│   ├── db.rs             # 连接池 + 迁移 + 演示种子数据
│   ├── models.rs         # 表模型 / 视图模型 / 请求体
│   ├── service.rs        # 业务核心：状态机、发布事务、门牌规则
│   ├── routes.rs         # Axum 路由
│   └── error.rs          # 统一错误 → HTTP 状态码
├── tests/api.rs          # 集成测试
├── frontend/             # Svelte + Vite
│   └── src/pages/        # 申请列表/新建/详情、前台导视、楼层导视、模拟接口
├── start.sh              # 一键启动
└── demo.sh               # 演示脚本（占用拦截 + 重复发布幂等）
```
