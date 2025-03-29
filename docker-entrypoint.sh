#!/bin/sh
set -e

# Redisを起動（バックグラウンド）
redis-server --daemonize yes

# NGINXを起動
echo "Starting NGINX..."
exec "$@"
