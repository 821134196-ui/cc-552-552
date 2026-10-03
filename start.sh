#!/usr/bin/env bash
# 写字楼门牌变更与引导更新系统 — 一键本地启动
# 用法: ./start.sh   （首次运行会安装前端依赖并构建，随后启动后端，访问 http://localhost:3000）
set -euo pipefail
cd "$(dirname "$0")"

export PATH="$HOME/.cargo/bin:$PATH"

echo "==> [1/3] 构建前端 (Svelte + Vite)"
if [ ! -d frontend/node_modules ]; then
  (cd frontend && npm install)
fi
(cd frontend && npm run build)

echo "==> [2/3] 编译后端 (Rust + Axum + SQLx/SQLite)"
cargo build

echo "==> [3/3] 启动服务  http://localhost:3000"
echo "    数据库文件: ./doorplate.db （首次启动自动建表并写入演示数据）"
exec cargo run
