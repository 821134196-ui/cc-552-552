# 写字楼租户门牌变更与引导更新系统

租户搬迁或更名后，物业用一套申请统一更新 **实体门牌、楼层导视、前台名册和快递地址**，
全部在同一个生效日切换；访客和快递员在切换当天即可按新导视找到正确位置。

- **后端**：Rust + Axum + SQLx（SQLite），见 `backend/`
- **前端**：Svelte 5 + Vite，见 `frontend/`
- **外部接口**：打印车间、快递网点均为**本地模拟接口**（仅落库一条调用记录，不外发）

## 一条命令启动

```bash
./run.sh
# 打开 http://127.0.0.1:8080
```

首次运行会自动安装前端依赖、编译 Rust 后端，然后启动服务并写入演示种子数据
（3 层楼、6 个门牌、3 家在租租户）。可用 `PORT=9000 ./run.sh` 改端口，
数据库默认在 `backend/data/doorplate.db`（删除即重置）。

前端开发模式（热更新，自动代理 /api 到 8080）：

```bash
cd frontend && npm install && npm run dev
```

## 业务流程与状态机

```
submitted 待审核
  └─ approve 审核（占用目标门牌；做三类拦截，见下）
       └─ fabricated 待安装（fabricate ok 记录制作合格 + 打印车间模拟下单）
            └─ installed 待验收（保存安装人、安装照片）
                 └─ accepted 待发布（保存验收人）
                      └─ published 已发布（同一生效日统一切换）
```

任一环节校验失败 → 申请进入 **blocked 已暂停**，同时保存暂停环节与原因，
修正资料后可重新提交。

### 暂停发布的三类原因

1. **目标门牌已被占用/预约**：房间在用且属于其他租户，或已被另一个进行中的申请预约
   （错误码 `room_occupied` / `room_already_reserved`，HTTP 409）。
2. **租约未生效**：租约生效日晚于统一切换日（`lease_not_effective`，409）。
3. **门牌制作有误**：制作结果录入不合格（`fabrication_failed`，409），重新制作合格后继续。

另：旧址发布后进入 **快件过渡转发期（14 天）**，期间寄往旧址的快件转发至新址，
但旧址 **不能再被用作新租户地址**（`room_in_transition`，409）。

## 一致性保障

- **防两个申请同时占用同一门牌**：数据库部分唯一索引
  `uq_application_active_target ON applications(new_room_id) WHERE status IN ('approved',...,'accepted')`，
  即使两个审核请求并发到达，数据库层也只允许一个成功。
- **重复发布幂等**：发布用条件更新（`status='accepted' AND published_once=0`）做 CAS，
  配合 `published_once` 闸门。重复点击/重复调用第二次直接返回 `idempotent: true`，
  **不会二次切换门牌，也不会重复触发打印/快递模拟通知**。
- 发布事务在单个 SQLite 事务内完成：旧址转过渡、新址入住、租户更新、
  快递改址与转发通知记录一起提交或一起回滚。

## 演示：占用拦截 + 重复发布只执行一次

```bash
./demo.sh
```

脚本用全新的临时数据库启动服务，自动演示：

- **场景 A**：两个申请争抢 401，第二个审核被 409 拦截，申请置为 blocked 并保存原因；
  另演示租约晚于切换日被拦截。
- **场景 B**：完整办理一户搬迁并发布；**连续发布两次**，第二次 `switched=false`，
  `publishedAt` 不变，模拟通知始终只有 3 条（打印 1、快递改址 1、旧址转发 1）。
- **附加**：发布后旧址 302 进入过渡期，再拿它当新租户地址提交审核同样被 409 拦截。

## 主要 API

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/guide` | 楼层导视 + 进行中变更（旧址/新址/切换日） |
| GET | `/api/applications` | 申请列表 |
| POST | `/api/applications` | 新建申请 |
| GET / PATCH | `/api/applications/{id}` | 详情（含状态事件、模拟通知）/ 修正后重新提交 |
| POST | `/api/applications/{id}/approve` | 审核并预约门牌 |
| POST | `/api/applications/{id}/fabricate` | 录入制作结果 `{ok,note}` |
| POST | `/api/applications/{id}/install` | 安装完成 `{installer,installPhoto}` |
| POST | `/api/applications/{id}/accept` | 验收 `{acceptedBy}` |
| POST | `/api/applications/{id}/publish` | 统一切换（幂等，返回 `{switched,idempotent}`） |
| GET | `/api/notifications` | 打印/快递模拟接口调用记录 |
