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
REQUESTS=15  # 送信するリクエスト数
WAIT_TIME=0.1  # リクエスト間の待機時間（秒）
API_KEY="test-api-key"  # APIキーのデフォルト値

# 使用方法を表示
function show_usage {
  echo "使用方法: $0 [オプション]"
  echo "オプション:"
  echo "  -h, --host      ホスト名またはIPアドレス (デフォルト: localhost)"
  echo "  -p, --port      ポート番号 (デフォルト: 8080)"
  echo "  -n, --requests  送信するリクエスト数 (デフォルト: 15)"
  echo "  -w, --wait      リクエスト間の待機時間（秒） (デフォルト: 0.1)"
  echo "  -k, --key       APIキー (デフォルト: test-api-key)"
  echo "  --help          このヘルプメッセージを表示"
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
    -n|--requests)
      REQUESTS="$2"
      shift 2
      ;;
    -w|--wait)
      WAIT_TIME="$2"
      shift 2
      ;;
    -k|--key)
      API_KEY="$2"
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

BASE_URL="http://${HOST}:${PORT}"

# テスト関数 - リクエストを送信して結果をカウント
function run_test {
  local endpoint=$1
  local header=$2
  local title=$3
  local count_success=0
  local count_limited=0
  local count_other=0

  echo -e "${BLUE}=== $title ===${NC}"
  echo "エンドポイント: $endpoint"
  echo "ヘッダー: $header"
  echo "リクエスト数: $REQUESTS"
  echo -e "リクエスト間隔: ${WAIT_TIME}秒\n"

  for ((i=1; i<=$REQUESTS; i++)); do
    echo -n "リクエスト $i: "

    # ヘッダーの有無に応じてcurlコマンドを構築
    if [ -z "$header" ]; then
      response=$(curl -s -w "%{http_code}" -o /tmp/curl_body.txt "${BASE_URL}${endpoint}")
    else
      response=$(curl -s -w "%{http_code}" -o /tmp/curl_body.txt -H "$header" "${BASE_URL}${endpoint}")
    fi

    body=$(cat /tmp/curl_body.txt)

    # レスポンスコードに応じた処理
    if [ "$response" == "200" ]; then
      echo -e "${GREEN}OK (200)${NC}"
      ((count_success++))
    elif [ "$response" == "403" ]; then
      echo -e "${RED}レート制限 (403)${NC}"
      echo -e "  ${YELLOW}レスポンス: $body${NC}"
      # レート制限ヘッダーがあれば表示
      curl -s -I -H "$header" "${BASE_URL}${endpoint}" | grep -i "X-RateLimit" || true
      ((count_limited++))
    else
      echo -e "${YELLOW}その他 ($response)${NC}"
      echo -e "  ${YELLOW}レスポンス: $body${NC}"
      ((count_other++))
    fi

    # 指定された時間だけ待機
    sleep $WAIT_TIME
  done

  echo -e "\n${BLUE}結果:${NC}"
  echo "成功: $count_success"
  echo "レート制限: $count_limited"
  echo "その他: $count_other"
  echo -e "${BLUE}==============================${NC}\n"
}

# メインテスト
echo -e "${BLUE}=====================================${NC}"
echo -e "${BLUE}Redisレートリミットテスト${NC}"
echo -e "${BLUE}=====================================${NC}\n"

# テスト1: ルートパスへのリクエスト（IPベースのレート制限）
run_test "/" "" "IPベースのレート制限テスト"

# テスト2: APIエンドポイントへのリクエスト（APIキーベースのレート制限）
run_test "/api" "X-API-Key: ${API_KEY}" "APIキーベースのレート制限テスト"

# テスト3: 静的リソースへのリクエスト（レート制限なし）
run_test "/static" "" "レート制限なしのテスト"

# テスト4: 異なるAPIキーでのテスト
run_test "/api" "X-API-Key: another-${API_KEY}" "異なるAPIキーでのテスト"

echo -e "${GREEN}テスト完了${NC}"
