#!/bin/bash

# カラー表示用の設定
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# コンテナ名を設定
CONTAINER_NAME="ngx-ratelimit-redis-test"

# イメージ名を設定
IMAGE_NAME="ngx-ratelimit-redis"

echo -e "${BLUE}=====================================${NC}"
echo -e "${BLUE}Redisレートリミット Dockerテスト${NC}"
echo -e "${BLUE}=====================================${NC}\n"

# Dockerイメージのビルド
echo -e "${BLUE}Dockerイメージをビルドしています...${NC}"
docker build -t ${IMAGE_NAME} .

# 古いコンテナを停止して削除
echo -e "\n${BLUE}既存のテストコンテナがあれば停止して削除します...${NC}"
docker stop ${CONTAINER_NAME} 2>/dev/null || true
docker rm ${CONTAINER_NAME} 2>/dev/null || true

# 新しいコンテナを起動
echo -e "\n${BLUE}テストコンテナを起動しています...${NC}"
docker run -d --name ${CONTAINER_NAME} -p 8080:8080 ${IMAGE_NAME}

# コンテナが起動するまで待機
echo -e "\n${BLUE}NGINXの起動を待機しています...${NC}"
sleep 5

# 初期テスト - サーバーが応答するか確認
echo -e "\n${BLUE}サーバーが応答するか確認しています...${NC}"
if curl -s -o /dev/null -w "%{http_code}" http://localhost:8080/ > /dev/null; then
  echo -e "${GREEN}サーバーは正常に応答しています${NC}"
else
  echo -e "${RED}サーバーからの応答がありません。コンテナログを確認してください:${NC}"
  docker logs ${CONTAINER_NAME}
  exit 1
fi

# テストスクリプトを実行
echo -e "\n${BLUE}基本的なレートリミットテストを実行しています...${NC}"
./script/test_rate_limit.sh -n 20 -w 0.1

# コンテナを停止しないオプション
if [ "$1" = "--keep" ]; then
  echo -e "\n${GREEN}テスト完了。コンテナは実行されたままです。${NC}"
  echo "コンテナを停止するには次のコマンドを使用してください: docker stop ${CONTAINER_NAME}"
else
  # テスト後にコンテナを停止
  echo -e "\n${BLUE}テストコンテナを停止しています...${NC}"
  docker stop ${CONTAINER_NAME}
  echo -e "${GREEN}テスト完了。コンテナは停止しました。${NC}"
fi
