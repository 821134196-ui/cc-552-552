#!/usr/bin/env bash
# 一条本地启动命令：准备 Rust/前端依赖 -> 构建 -> 启动服务。
# 用法：./run.sh          （首次会自动 npm install + cargo build）
#       PORT=9000 ./run.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND="$ROOT/backend"
FRONTEND="$ROOT/frontend"
PORT="${PORT:-8080}"

# 1) Rust 工具链
if ! command -v cargo >/dev/null 2>&1; then
  if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
  else
    echo "未找到 cargo，正在安装 Rust 工具链…"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
  fi
fi

# 2) 前端依赖与构建
if [ ! -d "$FRONTEND/node_modules" ]; then
  echo "安装前端依赖…"
  (cd "$FRONTEND" && npm install)
fi
echo "构建前端（Svelte + Vite）…"
(cd "$FRONTEND" && npm run build)

# 3) 后端构建
echo "构建后端（Rust + Axum）…"
(cd "$BACKEND" && cargo build)

# 4) 启动（数据库默认 backend/data/doorplate.db，首次启动写入演示种子数据）
echo "启动服务…"
cd "$BACKEND"
FRONTEND_DIR="$FRONTEND/dist" PORT="$PORT" \
  exec ./target/debug/doorplate-server
