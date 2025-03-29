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
CONCURRENCY=10      # 同時接続数
REQUESTS=100        # 総リクエスト数
API_KEY="test-api-key"
ENDPOINT="/"        # テスト対象のエンドポイント
USE_HEADER=false    # APIキーヘッダーを使用するかどうか

# 使用方法を表示
function show_usage {
  echo "使用方法: $0 [オプション]"
  echo "オプション:"
  echo "  -h, --host        ホスト名またはIPアドレス (デフォルト: localhost)"
  echo "  -p, --port        ポート番号 (デフォルト: 8080)"
  echo "  -c, --concurrency 同時接続数 (デフォルト: 10)"
  echo "  -n, --requests    総リクエスト数 (デフォルト: 100)"
  echo "  -k, --key         APIキー (デフォルト: test-api-key)"
  echo "  -e, --endpoint    テスト対象のエンドポイント (デフォルト: /)"
  echo "  --api             APIキーヘッダーを使用する"
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
    -c|--concurrency)
      CONCURRENCY="$2"
      shift 2
      ;;
    -n|--requests)
      REQUESTS="$2"
      shift 2
      ;;
    -k|--key)
      API_KEY="$2"
      shift 2
      ;;
    -e|--endpoint)
      ENDPOINT="$2"
      shift 2
      ;;
    --api)
      USE_HEADER=true
      shift
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

URL="http://${HOST}:${PORT}${ENDPOINT}"

# ab（Apache Bench）コマンドがインストールされているか確認
if ! command -v ab &> /dev/null; then
  echo -e "${RED}エラー: Apache Bench (ab) がインストールされていません。${NC}"
  echo "Ubuntuの場合: sudo apt-get install apache2-utils"
  echo "CentOSの場合: sudo yum install httpd-tools"
  echo "macOSの場合: brew install apr-util"
  exit 1
fi

# hey（HTTP負荷ツール）コマンドがインストールされているか確認
if ! command -v hey &> /dev/null; then
  echo -e "${YELLOW}注意: hey がインストールされていません。heyによるテストはスキップされます。${NC}"
  echo "インストール方法: go install github.com/rakyll/hey@latest"
  HEY_AVAILABLE=false
else
  HEY_AVAILABLE=true
fi

echo -e "${BLUE}=====================================${NC}"
echo -e "${BLUE}Redisレートリミットベンチマーク${NC}"
echo -e "${BLUE}=====================================${NC}\n"

echo "URL: $URL"
echo "同時接続数: $CONCURRENCY"
echo "総リクエスト数: $REQUESTS"
if [ "$USE_HEADER" = true ]; then
  echo "APIキー: $API_KEY"
fi
echo ""

# Apache Benchによるテスト
echo -e "${BLUE}=== Apache Benchによるテスト ===${NC}"
if [ "$USE_HEADER" = true ]; then
  echo "APIキーヘッダーを使用"
  ab -c $CONCURRENCY -n $REQUESTS -H "X-API-Key: $API_KEY" "$URL"
else
  ab -c $CONCURRENCY -n $REQUESTS "$URL"
fi

# heyによるテスト（インストールされている場合）
if [ "$HEY_AVAILABLE" = true ]; then
  echo -e "\n${BLUE}=== heyによるテスト ===${NC}"
  if [ "$USE_HEADER" = true ]; then
    echo "APIキーヘッダーを使用"
    hey -n $REQUESTS -c $CONCURRENCY -H "X-API-Key: $API_KEY" "$URL"
  else
    hey -n $REQUESTS -c $CONCURRENCY "$URL"
  fi
fi

echo -e "\n${GREEN}ベンチマーク完了${NC}"

# 結果の解釈と注意点を表示
echo -e "\n${BLUE}=== 結果の解釈 ===${NC}"
echo "1. レート制限が有効な場合、非200レスポンスが多数あるのは正常です"
echo "2. ベンチマーク中の失敗（非200レスポンス）はレート制限が機能していることを示します"
echo "3. レスポンスタイムの中央値が低いことが重要です"
echo "4. レート制限テストでは、ベンチマークツールの結果は通常の使用とは異なります"
echo "5. 実際のトラフィックパターンとは異なるため、参考値として扱ってください"
