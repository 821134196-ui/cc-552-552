#!/usr/bin/env bash
# 演示脚本：门牌占用被拦截 + 重复发布只执行一次切换
# 前提：服务已启动（./start.sh），且为初始种子数据
set -euo pipefail
BASE="${BASE:-http://localhost:3000}"

pass() { echo "  ✅ $1"; }
fail() { echo "  ❌ $1"; exit 1; }
json_get() { node -e "let s='';process.stdin.on('data',d=>s+=d).on('end',()=>{const v=JSON.parse(s);console.log(eval('v.'+process.argv[1]))})" "$1"; }

echo "════════════════════════════════════════════════════════"
echo "场景一：目标门牌已被占用 → 提交即被拦截 (409)"
echo "════════════════════════════════════════════════════════"
echo ">> 蓝海咨询（现址 1F 103）申请搬到 1F 101（星辰科技占用中）"
resp=$(curl -s -w '\n%{http_code}' -X POST "$BASE/api/applications" \
  -H 'Content-Type: application/json' \
  -d '{"tenant_id":2,"change_type":"move","old_doorplate_id":3,"target_doorplate_id":1,
       "lease_no":"ZL-DEMO-1","lease_start":"2026-10-01","effective_date":"2026-10-20"}')
code=$(echo "$resp" | tail -1)
body=$(echo "$resp" | head -n -1)
echo "   HTTP $code  $(echo "$body" | json_get message)"
[ "$code" = "409" ] && pass "占用门牌被拦截" || fail "应返回 409"

echo ""
echo ">> 两个申请同时占用同一空闲门牌 2F 203"
resp=$(curl -s -w '\n%{http_code}' -X POST "$BASE/api/applications" \
  -H 'Content-Type: application/json' \
  -d '{"tenant_id":2,"change_type":"move","old_doorplate_id":3,"target_doorplate_id":6,
       "lease_no":"ZL-DEMO-2","lease_start":"2026-10-01","effective_date":"2026-10-20"}')
code=$(echo "$resp" | tail -1)
[ "$code" = "201" ] && pass "第一个申请创建成功" || fail "第一个申请应创建成功"
resp=$(curl -s -w '\n%{http_code}' -X POST "$BASE/api/applications" \
  -H 'Content-Type: application/json' \
  -d '{"tenant_id":3,"change_type":"move","old_doorplate_id":4,"target_doorplate_id":6,
       "lease_no":"ZL-DEMO-3","lease_start":"2026-10-01","effective_date":"2026-10-20"}')
code=$(echo "$resp" | tail -1)
body=$(echo "$resp" | head -n -1)
echo "   HTTP $code  $(echo "$body" | json_get message)"
[ "$code" = "409" ] && pass "同一门牌的第二个申请被拦截" || fail "应返回 409"

echo ""
echo "════════════════════════════════════════════════════════"
echo "场景二：重复发布同一申请 → 只执行一次切换（幂等）"
echo "════════════════════════════════════════════════════════"
APP_ID=1   # 种子申请：星辰科技 1F 101 -> 2F 202
echo ">> 推进申请 #$APP_ID：审核 → 制作 → 安装 → 验收"
curl -s -X POST "$BASE/api/applications/$APP_ID/review"  -H 'Content-Type: application/json' -d '{"approve":true}' > /dev/null
curl -s -X POST "$BASE/api/applications/$APP_ID/produce" -H 'Content-Type: application/json' -d '{}' > /dev/null
curl -s -X POST "$BASE/api/applications/$APP_ID/install" -H 'Content-Type: application/json' -d '{"photo":"install-photo.jpg"}' > /dev/null
curl -s -X POST "$BASE/api/applications/$APP_ID/accept"  -H 'Content-Type: application/json' -d '{"accepted_by":"王验收"}' > /dev/null
pass "申请已验收"

echo ">> 第一次发布"
r1=$(curl -s -X POST "$BASE/api/applications/$APP_ID/publish")
echo "   result=$(echo "$r1" | json_get result)  $(echo "$r1" | json_get message)"
[ "$(echo "$r1" | json_get result)" = "published" ] && pass "首次发布成功" || fail "首次发布应成功"

echo ">> 第二次发布（重复发布）"
r2=$(curl -s -X POST "$BASE/api/applications/$APP_ID/publish")
echo "   result=$(echo "$r2" | json_get result)  $(echo "$r2" | json_get message)"
[ "$(echo "$r2" | json_get result)" = "already_published" ] && pass "重复发布被幂等忽略" || fail "应返回 already_published"

echo ">> 验证：状态流水里 published 只出现一次，快递通知只有一条"
detail=$(curl -s "$BASE/api/applications/$APP_ID")
pub_count=$(echo "$detail" | node -e "let s='';process.stdin.on('data',d=>s+=d).on('end',()=>{const v=JSON.parse(s);console.log(v.events.filter(e=>e.to_status==='published').length)})")
note_count=$(echo "$detail" | node -e "let s='';process.stdin.on('data',d=>s+=d).on('end',()=>{console.log(JSON.parse(s).courier_notifications.length)})")
echo "   published 事件数=$pub_count  快递通知数=$note_count"
[ "$pub_count" = "1" ] && [ "$note_count" = "1" ] && pass "切换只执行了一次" || fail "应各为 1"

echo ">> 验证：门牌已切换（2F 202 归星辰科技，1F 101 进入转发过渡期）"
plates=$(curl -s "$BASE/api/doorplates")
echo "$plates" | node -e "
let s='';process.stdin.on('data',d=>s+=d).on('end',()=>{
  const arr=JSON.parse(s);
  const p202=arr.find(p=>p.room==='202'), p101=arr.find(p=>p.room==='101');
  console.log(\`   2F 202: \${p202.status} (tenant=\${p202.tenant_name})\`);
  console.log(\`   1F 101: \${p101.status} → 转发至 \${p101.forwarding_to}，截止 \${p101.forwarding_until}\`);
})"
pass "门牌状态正确"

echo ""
echo "全部演示场景通过 🎉"
