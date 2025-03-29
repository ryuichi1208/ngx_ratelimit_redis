FROM rust:1.71-buster as builder

# 依存パッケージのインストール
RUN apt-get update && apt-get install -y \
    build-essential \
    libpcre3-dev \
    zlib1g-dev \
    libssl-dev \
    git \
    curl \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# ソースのコピー
WORKDIR /usr/src/app
COPY . .

# ビルド
ARG NGX_VERSION=1.26.3
RUN NGX_VERSION=${NGX_VERSION} cargo build --release

# 実行環境
FROM nginx:1.26-alpine

# Redisをインストール
RUN apk add --no-cache redis

# モジュールをコピー
COPY --from=builder /usr/src/app/target/release/libngx_ratelimit_redis.so /usr/lib/nginx/modules/

# 設定ファイルをコピー
COPY nginx.conf.example /etc/nginx/nginx.conf

# 起動スクリプト
COPY docker-entrypoint.sh /
RUN chmod +x /docker-entrypoint.sh

EXPOSE 8080

ENTRYPOINT ["/docker-entrypoint.sh"]
CMD ["nginx", "-g", "daemon off;"]
