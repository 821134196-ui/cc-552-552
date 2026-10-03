#!/usr/bin/env bash
# 端到端演示：
#   A. 目标门牌已被占用/预约 -> 第二个申请审核被拦截（409，申请置为 blocked 并记录原因）
#   B. 完整走查 审核->制作->安装->验收->发布，重复发布只执行一次更新（幂等）
#   C. 附加：租约未生效被拦截；旧址进入快件过渡期，不能再用作新租户地址
#
# 脚本会自行在临时端口启动一个使用全新数据库的服务，演示结束自动清理。
# 用法：./demo.sh
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PORT="${DEMO_PORT:-8081}"
BASE="http://127.0.0.1:$PORT"
DB="/tmp/doorplate-demo-$$.db"

# shellcheck disable=SC1091
[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"

echo ">> 构建后端…"
(cd "$ROOT/backend" && cargo build -q)

echo ">> 以全新数据库启动演示服务（端口 $PORT）…"
rm -f "$DB" "$DB-wal" "$DB-shm"
(
  cd "$ROOT/backend"
  DOORPLATE_DB="$DB" FRONTEND_DIR="$ROOT/frontend/dist" PORT="$PORT" \
    ./target/debug/doorplate-server >/tmp/doorplate-demo-server.log 2>&1
) &
SERVER_PID=$!
cleanup() { kill "$SERVER_PID" 2>/dev/null || true; rm -f "$DB" "$DB-wal" "$DB-shm"; }
trap cleanup EXIT

for _ in $(seq 1 50); do
  curl -sf "$BASE/api/health" >/dev/null 2>&1 && break
  sleep 0.1
done

# 简单 JSON 取值：jget <json> <python表达式，a为解析后的对象>
jget() { python3 -c "import json,sys; a=json.loads(sys.argv[1]); print($2)" "$1"; }
# 调用 API 并打印 HTTP 状态码，响应体存入 RESP
call() {
  local method=$1 path=$2 body=${3-}
  if [ -n "$body" ]; then
    RESP="$(curl -s -X "$method" "$BASE$path" -H 'Content-Type: application/json' -d "$body" -w $'\n%{http_code}')"
  else
    RESP="$(curl -s -X "$method" "$BASE$path" -w $'\n%{http_code}')"
  fi
  CODE="${RESP##*$'\n'}"
  BODY="${RESP%$'\n'*}"
}
hr() { echo; echo "──────── $1 ────────"; }

hr "场景 A：门牌占用拦截（两个申请争抢 401）"
A=$(curl -s -X POST "$BASE/api/applications" -H 'Content-Type: application/json' -d '{
  "tenantName":"云岚科技有限公司","tenantId":1,"oldRoomId":3,"newRoomId":5,
  "leaseRef":"L-2026-001","leaseEffectiveDate":"2026-09-20","effectiveDate":"2026-10-10"}')
A_ID=$(jget "$A" "a['application']['id']")
echo "申请1 编号=$(jget "$A" "a['application']['code']")  云岚科技 301 → 401"

call POST "/api/applications/$A_ID/approve"
echo "审核申请1：HTTP $CODE，状态=$(jget "$BODY" "a['application']['status']")（401 已预约）"

B=$(curl -s -X POST "$BASE/api/applications" -H 'Content-Type: application/json' -d '{
  "tenantName":"瀚海智能公司（新租户）","newRoomId":5,
  "leaseRef":"L-2026-002","leaseEffectiveDate":"2026-10-01","effectiveDate":"2026-10-15"}')
B_ID=$(jget "$B" "a['application']['id']")
echo "申请2 编号=$(jget "$B" "a['application']['code']")  瀚海智能 → 同一门牌 401"

call POST "/api/applications/$B_ID/approve"
echo "审核申请2：HTTP $CODE  ← 被拦截"
echo "  错误码：$(jget "$BODY" "a['error']")"
echo "  原因：  $(jget "$BODY" "a['message']")"
B_AFTER=$(curl -s "$BASE/api/applications/$B_ID")
echo "  申请2 落库状态=$(jget "$B_AFTER" "a['application']['status']")，暂停原因已保存："
echo "    $(jget "$B_AFTER" "a['application']['blockReason']")"

hr "附加：租约未生效拦截（切换日早于租约生效日）"
C=$(curl -s -X POST "$BASE/api/applications" -H 'Content-Type: application/json' -d '{
  "tenantName":"晚到租户","newRoomId":2,
  "leaseRef":"L-2026-009","leaseEffectiveDate":"2026-11-01","effectiveDate":"2026-10-20"}')
C_ID=$(jget "$C" "a['application']['id']")
call POST "/api/applications/$C_ID/approve"
echo "审核：HTTP $CODE，$(jget "$BODY" "a['message']")"

hr "场景 B：完整办理 + 重复发布只执行一次"
D=$(curl -s -X POST "$BASE/api/applications" -H 'Content-Type: application/json' -d '{
  "tenantName":"北辰工业设计事务所","tenantId":2,"oldRoomId":4,"newRoomId":6,
  "leaseRef":"L-2026-010","leaseEffectiveDate":"2026-09-01","effectiveDate":"2026-10-08"}')
D_ID=$(jget "$D" "a['application']['id']")
echo "申请 北辰设计 302 → 402（id=$D_ID）"

call POST "/api/applications/$D_ID/approve";            echo "审核     -> HTTP $CODE"
call POST "/api/applications/$D_ID/fabricate" '{"ok":true,"note":"实体门牌与导视牌制作合格"}'
echo "制作合格 -> HTTP $CODE（已向打印车间模拟接口下单）"
call POST "/api/applications/$D_ID/install" '{"installer":"王工","installPhoto":"photos/402-20261008.jpg"}'
echo "安装     -> HTTP $CODE（安装人/照片已存入申请）"
call POST "/api/applications/$D_ID/accept" '{"acceptedBy":"物业经理 李珊"}'
echo "验收     -> HTTP $CODE（验收人已存入申请）"

call POST "/api/applications/$D_ID/publish"
echo "第一次发布 -> HTTP $CODE  switched=$(jget "$BODY" "a['switched']")  idempotent=$(jget "$BODY" "a['idempotent']")"
PUB1_AT="$(jget "$BODY" "a['application']['publishedAt']")"

call POST "/api/applications/$D_ID/publish"
echo "重复发布   -> HTTP $CODE  switched=$(jget "$BODY" "a['switched']")  idempotent=$(jget "$BODY" "a['idempotent']")"
PUB2_AT="$(jget "$BODY" "a['application']['publishedAt']")"
[ "$PUB1_AT" = "$PUB2_AT" ] && echo "  publishedAt 未改变（$PUB1_AT）→ 没有发生第二次切换"

D_DETAIL=$(curl -s "$BASE/api/applications/$D_ID")
echo "  该申请的模拟接口通知条数=$(jget "$D_DETAIL" "len(a['application']['notifications'])")（打印1+快递改址1+旧址转发1，重复发布未新增）："
jget "$D_DETAIL" "
'\n'.join('    - %s %s -> %s' % (n['kind'], n['target'].split('/')[-1], n['result'])
          for n in a['application']['notifications'])"

hr "发布后状态：新址入住，旧址仅转发快件"
curl -s "$BASE/api/guide" | python3 -c "
import json,sys
d=json.load(sys.stdin)
for f in d['floors']:
    for r in f['rooms']:
        if r['code'] in ('302','402'):
            print(f\"  {r['code']}: {r['status']}  租户={r.get('tenantName')}  转发至={r.get('forwardToRoomCode')}  截止={r.get('forwardUntil')}\")"

hr "附加：过渡期旧址 302 不能再用作新租户地址"
E=$(curl -s -X POST "$BASE/api/applications" -H 'Content-Type: application/json' -d '{
  "tenantName":"误用旧址者","newRoomId":4,
  "leaseRef":"L-X","leaseEffectiveDate":"2026-10-01","effectiveDate":"2026-10-09"}')
E_ID=$(jget "$E" "a['application']['id']")
call POST "/api/applications/$E_ID/approve"
echo "审核：HTTP $CODE，$(jget "$BODY" "a['message']")"

echo
echo "✔ 演示完成：门牌占用/租约拦截均返回 409 并暂停申请；重复发布幂等，只执行一次更新。"
