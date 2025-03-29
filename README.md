# ngx_ratelimit_redis

NGINXでRedisをバックエンドとして使用するレートリミットモジュール。[ngx-rust](https://github.com/nginx/ngx-rust)を使用してRustで実装されています。

## 機能

- Redisをバックエンドとした分散レートリミット
- IPアドレスやカスタムヘッダー（APIキーなど）に基づいたレート制限
- 毎秒のリクエスト数とバースト値の設定
- スライディングウィンドウアルゴリズムによる正確なレート制限

## ビルド方法

### 依存関係

- Rust（1.65以上）
- NGINX（1.22.0以上推奨）
- Cargo
- Redis（4.0以上）

### コンパイル

```bash
# 環境変数でNGINXのバージョンを指定可能
NGX_VERSION=1.26.3 cargo build --release
```

ビルドが完了すると、`target/release/libngx_ratelimit_redis.so`（Linux）または`target/release/libngx_ratelimit_redis.dylib`（MacOS）が生成されます。

### Dockerを使用したビルド

```bash
# Dockerイメージをビルド
docker build -t ngx-ratelimit-redis .

# コンテナを起動
docker run -d -p 8080:8080 ngx-ratelimit-redis
```

## インストール

生成されたモジュールファイルをNGINXのモジュールディレクトリにコピーします：

```bash
# Linuxの場合
sudo cp target/release/libngx_ratelimit_redis.so /usr/lib/nginx/modules/

# MacOSの場合
sudo cp target/release/libngx_ratelimit_redis.dylib /usr/local/opt/nginx/modules/
```

## 設定

NGINXの設定ファイルでモジュールをロードし、設定を行います：

```nginx
# モジュールをロード
load_module modules/libngx_ratelimit_redis.so;

http {
    server {
        # ...

        location / {
            # モジュールを有効化
            ratelimit_redis on redis_url=redis://127.0.0.1:6379 key=remote_addr rate=10 burst=5;

            # 他の設定...
        }
    }
}
```

### 設定オプション

| オプション  | 説明                                   | デフォルト値              |
|-----------|----------------------------------------|-------------------------|
| on/off    | モジュールの有効/無効                    | off                     |
| redis_url | Redisサーバーの接続URL                  | redis://127.0.0.1:6379  |
| key       | レート制限に使用するキー                  | remote_addr             |
| rate      | 1秒あたりの最大リクエスト数               | 10                      |
| burst     | 一時的に許容される超過リクエスト数         | 5                       |

### キーの種類

- `remote_addr`: クライアントのIPアドレス
- `http_[ヘッダー名]`: 指定したHTTPヘッダーの値（例: `http_x_api_key`）

## 使用例

```nginx
# IPアドレスベースのレート制限
location / {
    ratelimit_redis on redis_url=redis://127.0.0.1:6379 key=remote_addr rate=10 burst=5;
    # ...
}

# APIキーベースのレート制限
location /api {
    ratelimit_redis on redis_url=redis://redis-server:6379 key=http_x_api_key rate=5 burst=2;
    # ...
}

# レート制限を無効化
location /static {
    ratelimit_redis off;
    # ...
}
```

## テスト

このモジュールの動作を検証するためのテストスクリプトが `script` ディレクトリに用意されています。

### 基本的なテスト

```bash
# 基本的なレート制限のテスト
./script/test_rate_limit.sh

# カスタム設定でテスト
./script/test_rate_limit.sh -n 30 -w 0.2
```

### Dockerを使用したテスト

```bash
# Dockerでのテスト（イメージのビルド、コンテナ起動、テスト実行を自動化）
./script/docker_test.sh

# テスト後もコンテナを起動したままにする場合
./script/docker_test.sh --keep
```

### ベンチマーク

```bash
# レート制限機能のベンチマーク
./script/benchmark_rate_limit.sh

# APIキーを使用したベンチマーク
./script/benchmark_rate_limit.sh --api -e /api
```

詳細なテスト方法については、[script/README.md](script/README.md) を参照してください。

## 動作の仕組み

このモジュールは、Redisをバックエンドとして使用し、スライディングウィンドウアルゴリズムでレート制限を実装しています。これにより、分散環境でも正確なレート制限が可能になります。

1. リクエストが来るとキー（IPアドレスやAPIキーなど）を抽出
2. Redisでそのキーのカウンターを増加
3. 設定された制限を超えた場合、403 Forbiddenを返す
4. 制限内の場合、リクエスト処理を続行

## ライセンス

Apache License 2.0

## 参考

- [NGINX](https://nginx.org/)
- [ngx-rust](https://github.com/nginx/ngx-rust)
- [Redis](https://redis.io/)
