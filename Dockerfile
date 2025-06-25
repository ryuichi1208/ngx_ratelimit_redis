FROM rust:latest as builder

# 依存パッケージのインストール
RUN apt-get update && apt-get install -y \
    build-essential \
    libpcre3-dev \
    zlib1g-dev \
    libssl-dev \
    git \
    curl \
    pkg-config \
    llvm-dev \
    libclang-dev \
    clang \
    && rm -rf /var/lib/apt/lists/*

# NGINX ソースコードをダウンロードしてヘッダーファイルを使えるようにする
ARG NGX_VERSION=1.26.3
RUN curl -sSL "https://nginx.org/download/nginx-${NGX_VERSION}.tar.gz" | tar xz

ENV NGINX_SOURCE="/usr/src/nginx-${NGX_VERSION}"
ENV NGINX_LIB="/usr/src/nginx-${NGX_VERSION}"
ENV LLVM_CONFIG_PATH=/usr/bin/llvm-config

# GPG検証をスキップする環境変数
ENV GNUPGHOME=/tmp/gnupg
RUN mkdir -p $GNUPGHOME && chmod 700 $GNUPGHOME

# ソースのコピー
WORKDIR /usr/src/app
COPY . .

# ビルド
ENV NGX_VERSION=${NGX_VERSION}
RUN cargo build --release

# 実行環境
FROM nginx:1.29-alpine

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
