# テストスクリプト

このディレクトリには、ngx_ratelimit_redisモジュールをテストするためのスクリプトが含まれています。

## スクリプト一覧

### test_rate_limit.sh

基本的なレート制限機能をテストするためのスクリプトです。curlを使用して指定された回数のリクエストを送信し、レート制限の動作を確認します。

```bash
./script/test_rate_limit.sh [オプション]
```

#### オプション:
- `-h, --host` - ホスト名またはIPアドレス (デフォルト: localhost)
- `-p, --port` - ポート番号 (デフォルト: 8080)
- `-n, --requests` - 送信するリクエスト数 (デフォルト: 15)
- `-w, --wait` - リクエスト間の待機時間（秒） (デフォルト: 0.1)
- `-k, --key` - APIキー (デフォルト: test-api-key)

#### 使用例:
```bash
# デフォルト設定でテスト実行
./script/test_rate_limit.sh

# カスタム設定でテスト実行
./script/test_rate_limit.sh -h 192.168.1.10 -p 80 -n 30 -w 0.5 -k my-api-key
```

### benchmark_rate_limit.sh

Apache Bench (ab) や hey などのツールを使用して、レート制限機能の性能をベンチマークするスクリプトです。

```bash
./script/benchmark_rate_limit.sh [オプション]
```

#### オプション:
- `-h, --host` - ホスト名またはIPアドレス (デフォルト: localhost)
- `-p, --port` - ポート番号 (デフォルト: 8080)
- `-c, --concurrency` - 同時接続数 (デフォルト: 10)
- `-n, --requests` - 総リクエスト数 (デフォルト: 100)
- `-k, --key` - APIキー (デフォルト: test-api-key)
- `-e, --endpoint` - テスト対象のエンドポイント (デフォルト: /)
- `--api` - APIキーヘッダーを使用する

#### 使用例:
```bash
# デフォルト設定でベンチマーク実行
./script/benchmark_rate_limit.sh

# APIキーを使用したベンチマーク
./script/benchmark_rate_limit.sh --api -e /api -n 500 -c 20
```

### docker_test.sh

Dockerを使用してモジュールをビルド・実行し、テストするためのスクリプトです。DockerイメージをビルドしてコンテナでNGINXを起動し、test_rate_limit.shを使用してテストを実行します。

```bash
./script/docker_test.sh [オプション]
```

#### オプション:
- `--keep` - テスト後もコンテナを実行したままにする

#### 使用例:
```bash
# Dockerを使用したテスト実行（テスト後にコンテナを停止）
./script/docker_test.sh

# テスト後もコンテナを実行したままにする
./script/docker_test.sh --keep
```

## 前提条件

- テストスクリプト: curlがインストールされていること
- ベンチマークスクリプト: Apache Bench (ab) がインストールされていること
- Dockerテストスクリプト: Dockerがインストールされていること

## 注意事項

- レート制限のテストでは、多数のリクエストが403 Forbiddenで失敗するのは正常な動作です。
- ベンチマークの結果は実際のトラフィックパターンとは異なるため、参考値として扱ってください。
- Dockerテスト時は、ポート8080がすでに使用されていないことを確認してください。
