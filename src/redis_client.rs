use log::{debug, error, info};
use redis::{aio::Connection, AsyncCommands, Client, RedisError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub redis_url: String,
    pub requests_per_second: u32,
    pub burst: u32,
}

pub struct RedisRateLimiter {
    client: Client,
    config: RateLimitConfig,
}

impl RedisRateLimiter {
    // 新しいRedisRateLimiterインスタンスを作成
    pub async fn new(config: RateLimitConfig) -> Result<Self, String> {
        info!("Connecting to Redis at: {}", config.redis_url);

        let client = match Client::open(config.redis_url.clone()) {
            Ok(client) => client,
            Err(err) => {
                error!("Failed to create Redis client: {}", err);
                return Err(format!("Failed to create Redis client: {}", err));
            }
        };

        // 接続テスト
        let mut conn = match client.get_async_connection().await {
            Ok(conn) => conn,
            Err(err) => {
                error!("Failed to connect to Redis: {}", err);
                return Err(format!("Failed to connect to Redis: {}", err));
            }
        };

        // PINGでRedisサーバーが応答するか確認
        match redis::cmd("PING").query_async::<_, String>(&mut conn).await {
            Ok(response) => {
                if response != "PONG" {
                    error!("Unexpected response from Redis server: {}", response);
                    return Err(format!(
                        "Unexpected response from Redis server: {}",
                        response
                    ));
                }
                info!("Successfully connected to Redis");
            }
            Err(err) => {
                error!("Failed to ping Redis server: {}", err);
                return Err(format!("Failed to ping Redis server: {}", err));
            }
        }

        Ok(RedisRateLimiter { client, config })
    }

    // 接続取得のヘルパーメソッド
    async fn get_connection(&self) -> Result<Connection, RedisError> {
        self.client.get_async_connection().await
    }

    // レートリミットのチェック
    // sliding window algorithmを使用
    pub async fn check_rate_limit(&self, key: &str) -> Result<bool, String> {
        let mut conn = match self.get_connection().await {
            Ok(conn) => conn,
            Err(err) => {
                error!("Failed to get Redis connection: {}", err);
                return Err(format!("Failed to get Redis connection: {}", err));
            }
        };

        // 現在のタイムスタンプ（秒）
        let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(n) => n.as_secs(),
            Err(_) => {
                error!("SystemTime before UNIX EPOCH!");
                return Err("SystemTime before UNIX EPOCH!".to_string());
            }
        };

        let window_size = 60; // 1分間のウィンドウサイズ
        let redis_key = format!("ratelimit:{}:{}", key, now / window_size * window_size);
        let expire_time = window_size * 2; // レコードの有効期限

        // LUAスクリプトを使用して、アトミックにレート制限をチェック
        let script = r#"
            local key = KEYS[1]
            local now = tonumber(ARGV[1])
            local window_size = tonumber(ARGV[2])
            local max_requests = tonumber(ARGV[3])
            local burst = tonumber(ARGV[4])
            local expire_time = tonumber(ARGV[5])

            -- 現在のカウントを取得
            local count = redis.call('INCR', key)

            -- 初回アクセスの場合、有効期限を設定
            if count == 1 then
                redis.call('EXPIRE', key, expire_time)
            end

            -- バーストを含む最大リクエスト数を超えたかチェック
            if count <= (max_requests + burst) then
                return 1  -- 許可
            else
                return 0  -- 拒否
            end
        "#;

        let result: Result<i32, RedisError> = redis::Script::new(script)
            .key(redis_key)
            .arg(now)
            .arg(window_size)
            .arg(self.config.requests_per_second)
            .arg(self.config.burst)
            .arg(expire_time)
            .invoke_async(&mut conn)
            .await;

        match result {
            Ok(val) => {
                debug!("Rate limit check for {}: {}", key, val);
                Ok(val == 1)
            }
            Err(err) => {
                error!("Failed to execute rate limit script: {}", err);
                Err(format!("Failed to execute rate limit script: {}", err))
            }
        }
    }
}
