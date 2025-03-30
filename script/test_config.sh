#!/bin/bash

# カラー表示用の設定
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# デフォルト設定
HOST="localhost"
PORT="8080"
CONFIG_FILE="config.json.example"
OUTPUT_DIR="test_configs"

# 使用方法を表示
function show_usage {
  echo "使用方法: $0 [オプション]"
  echo "オプション:"
  echo "  -h, --host        ホスト名またはIPアドレス (デフォルト: localhost)"
  echo "  -p, --port        ポート番号 (デフォルト: 8080)"
  echo "  -c, --config      設定ファイル (デフォルト: config.json.example)"
  echo "  -o, --output-dir  出力ディレクトリ (デフォルト: test_configs)"
  echo "  --help            このヘルプメッセージを表示"
  exit 1
}

# コマンドライン引数の解析
while [[ $# -gt 0 ]]; do
  case $1 in
    -h|--host)
      HOST="$2"
      shift 2
      ;;
    -p|--port)
      PORT="$2"
      shift 2
      ;;
    -c|--config)
      CONFIG_FILE="$2"
      shift 2
      ;;
    -o|--output-dir)
      OUTPUT_DIR="$2"
      shift 2
      ;;
    --help)
      show_usage
      ;;
    *)
      echo "不明なオプション: $1"
      show_usage
      ;;
  esac
done

# 出力ディレクトリの作成
mkdir -p "$OUTPUT_DIR"

echo -e "${BLUE}=====================================${NC}"
echo -e "${BLUE}設定ファイルテスト${NC}"
echo -e "${BLUE}=====================================${NC}\n"

echo "設定ファイル: $CONFIG_FILE"
echo -e "出力ディレクトリ: $OUTPUT_DIR\n"

# 設定ファイルの検証
if [ ! -f "$CONFIG_FILE" ]; then
  echo -e "${RED}エラー: 設定ファイル '$CONFIG_FILE' が見つかりません${NC}"
  exit 1
fi

# JSONの構文チェック
echo -e "${BLUE}JSONの構文チェック...${NC}"
if command -v jq &> /dev/null; then
  jq . "$CONFIG_FILE" > /dev/null
  if [ $? -eq 0 ]; then
    echo -e "${GREEN}JSONの構文は正常です${NC}"
    jq . "$CONFIG_FILE" > "$OUTPUT_DIR/formatted_config.json"
    echo -e "フォーマットされた設定ファイルを $OUTPUT_DIR/formatted_config.json に保存しました"
  else
    echo -e "${RED}JSONの構文エラーがあります${NC}"
    exit 1
  fi
else
  echo -e "${YELLOW}警告: jqコマンドがインストールされていないため、構文チェックをスキップします${NC}"
fi

# 設定内容の確認
echo -e "\n${BLUE}設定内容:${NC}"
if command -v jq &> /dev/null; then
  # デフォルト設定の表示
  echo -e "${YELLOW}デフォルト設定:${NC}"
  jq -r '.default | to_entries | .[] | "  \(.key): \(.value)"' "$CONFIG_FILE"

  # ロケーション設定の表示
  echo -e "\n${YELLOW}ロケーション設定:${NC}"
  jq -r '.locations | to_entries[] | "  \(.key):"' "$CONFIG_FILE" | while read -r location; do
    echo "$location"
    location_key=$(echo "$location" | sed 's/  \(.*\):/\1/')
    jq -r ".locations[\"$location_key\"] | to_entries | .[] | \"    \(.key): \(.value)\"" "$CONFIG_FILE"
  done
else
  echo -e "${YELLOW}jqコマンドがインストールされていないため、詳細な設定内容の表示をスキップします${NC}"
  cat "$CONFIG_FILE"
fi

# テスト用のNGINX設定ファイルを生成
NGINX_CONF="${OUTPUT_DIR}/nginx_test.conf"

echo -e "\n${BLUE}NGINXテスト設定ファイルを生成しています...${NC}"
cat > "$NGINX_CONF" << EOF
worker_processes 1;
error_log logs/error.log debug;
events {
    worker_connections 1024;
}

# Redisレートリミットモジュールをロード
load_module modules/libngx_ratelimit_redis.so;

http {
    # グローバル設定ファイルの指定
    ratelimit_redis_config $(realpath "$CONFIG_FILE");

    server {
        listen $PORT;
        server_name localhost;

        # 設定ファイルから読み込んだLocation
EOF

# 設定ファイルからLocationを取得して設定を生成
if command -v jq &> /dev/null; then
  jq -r '.locations | keys[]' "$CONFIG_FILE" | while read -r location; do
    cat >> "$NGINX_CONF" << EOF

        location $location {
            # 設定ファイルの設定が適用されます
            ratelimit_redis on;

            # レスポンスにLocationパスを含める
            return 200 '{"location": "$location", "config": "from_file"}';
        }
EOF
  done
else
  # jqがない場合はデフォルトの設定を追加
  cat >> "$NGINX_CONF" << EOF

        location / {
            ratelimit_redis on;
            return 200 '{"location": "/", "config": "from_file"}';
        }

        location /api {
            ratelimit_redis on;
            return 200 '{"location": "/api", "config": "from_file"}';
        }
EOF
fi

# 追加の設定
cat >> "$NGINX_CONF" << EOF

        # 直接設定するLocation
        location /direct {
            ratelimit_redis on redis_url=redis://127.0.0.1:6379 key=remote_addr rate=15 burst=3 algorithm=sliding_window;
            return 200 '{"location": "/direct", "config": "direct"}';
        }

        # 無効化するLocation
        location /disabled {
            ratelimit_redis off;
            return 200 '{"location": "/disabled", "config": "disabled"}';
        }
    }
}
EOF

echo -e "${GREEN}NGINXテスト設定ファイルを生成しました: $NGINX_CONF${NC}"

# テスト用のcURLコマンドを生成
CURL_TEST="${OUTPUT_DIR}/test_endpoints.sh"

echo -e "\n${BLUE}cURLテストスクリプトを生成しています...${NC}"
cat > "$CURL_TEST" << EOF
#!/bin/bash

# 各エンドポイントに対してcURLリクエストを送信するテストスクリプト
HOST="$HOST"
PORT="$PORT"

echo "=== テスト開始 ==="

EOF

# 設定ファイルからLocationを取得してテストコマンドを生成
if command -v jq &> /dev/null; then
  jq -r '.locations | keys[]' "$CONFIG_FILE" | while read -r location; do
    cat >> "$CURL_TEST" << EOF
echo "テスト: $location"
curl -s -X GET "http://\$HOST:\$PORT$location" | jq .
echo ""

EOF
  done
else
  # jqがない場合はデフォルトのテストを追加
  cat >> "$CURL_TEST" << EOF
echo "テスト: /"
curl -s -X GET "http://\$HOST:\$PORT/" | python -m json.tool
echo ""

echo "テスト: /api"
curl -s -X GET "http://\$HOST:\$PORT/api" -H "X-API-Key: test-key" | python -m json.tool
echo ""

EOF
fi

# 追加のテスト
cat >> "$CURL_TEST" << EOF
echo "テスト: /direct (直接設定)"
curl -s -X GET "http://\$HOST:\$PORT/direct" | jq .
echo ""

echo "テスト: /disabled (無効化)"
curl -s -X GET "http://\$HOST:\$PORT/disabled" | jq .
echo ""

echo "=== テスト終了 ==="
EOF

chmod +x "$CURL_TEST"
echo -e "${GREEN}cURLテストスクリプトを生成しました: $CURL_TEST${NC}"

echo -e "\n${BLUE}テストの実行方法:${NC}"
echo "1. NGINXを起動: nginx -c $(realpath "$NGINX_CONF")"
echo "2. テストを実行: $CURL_TEST"

echo -e "\n${GREEN}設定ファイルテスト完了${NC}"
