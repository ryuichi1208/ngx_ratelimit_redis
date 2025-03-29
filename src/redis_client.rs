use log::{debug, error, info};
use redis::{aio::Connection, AsyncCommands, Client, RedisError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// レート制限アルゴリズムの種類
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RateLimitAlgorithm {
    /// 固定ウィンドウ: 一定時間内のリクエスト数を制限する最も単純な方法
    FixedWindow,
    /// スライディングウィンドウ: 時間窓を細かく分割し、より均一なレート制限を提供
    SlidingWindow,
    /// トークンバケット: 一定レートでトークンがバケットに追加され、リクエストごとにトークンを消費
    TokenBucket,
    /// リーキーバケット: 一定レートでリクエストを処理し、超過リクエストはキューに入る
    LeakyBucket,
}

impl Default for RateLimitAlgorithm {
    fn default() -> Self {
        RateLimitAlgorithm::SlidingWindow
    }
}

impl std::fmt::Display for RateLimitAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RateLimitAlgorithm::FixedWindow => write!(f, "fixed_window"),
            RateLimitAlgorithm::SlidingWindow => write!(f, "sliding_window"),
            RateLimitAlgorithm::TokenBucket => write!(f, "token_bucket"),
            RateLimitAlgorithm::LeakyBucket => write!(f, "leaky_bucket"),
        }
    }
}

impl RateLimitAlgorithm {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "fixed_window" => Ok(RateLimitAlgorithm::FixedWindow),
            "sliding_window" => Ok(RateLimitAlgorithm::SlidingWindow),
            "token_bucket" => Ok(RateLimitAlgorithm::TokenBucket),
            "leaky_bucket" => Ok(RateLimitAlgorithm::LeakyBucket),
            _ => Err(format!("Unknown rate limit algorithm: {}", s)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub redis_url: String,
    pub requests_per_second: u32,
    pub burst: u32,
    pub algorithm: RateLimitAlgorithm,
    pub window_size: u32, // 秒単位のウィンドウサイズ（固定ウィンドウとスライディングウィンドウ用）
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            redis_url: "redis://127.0.0.1:6379".to_string(),
            requests_per_second: 10,
            burst: 5,
            algorithm: RateLimitAlgorithm::SlidingWindow,
            window_size: 60, // デフォルトは1分
        }
    }
}

pub struct RedisRateLimiter {
    client: Client,
    config: RateLimitConfig,
}

impl RedisRateLimiter {
    // 新しいRedisRateLimiterインスタンスを作成
    pub async fn new(config: RateLimitConfig) -> Result<Self, String> {
        info!("Connecting to Redis at: {}", config.redis_url);
        info!("Using rate limit algorithm: {}", config.algorithm);

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
    pub async fn check_rate_limit(&self, key: &str) -> Result<bool, String> {
        match self.config.algorithm {
            RateLimitAlgorithm::FixedWindow => self.check_fixed_window(key).await,
            RateLimitAlgorithm::SlidingWindow => self.check_sliding_window(key).await,
            RateLimitAlgorithm::TokenBucket => self.check_token_bucket(key).await,
            RateLimitAlgorithm::LeakyBucket => self.check_leaky_bucket(key).await,
        }
    }

    // 固定ウィンドウアルゴリズム
    async fn check_fixed_window(&self, key: &str) -> Result<bool, String> {
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

        // 現在のウィンドウの開始時間を計算
        let window_size = self.config.window_size as u64;
        let window_start = (now / window_size) * window_size;
        let redis_key = format!("ratelimit:fixed:{}:{}", key, window_start);

        // LUAスクリプトを使用して、アトミックにレート制限をチェック
        let script = r#"
            local key = KEYS[1]
            local max_requests = tonumber(ARGV[1])
            local window_size = tonumber(ARGV[2])

            -- 現在のカウントを取得
            local count = redis.call('INCR', key)

            -- 初回アクセスの場合、有効期限を設定
            if count == 1 then
                redis.call('EXPIRE', key, window_size)
            end

            -- リクエスト数が制限以下かチェック
            if count <= max_requests then
                return 1  -- 許可
            else
                return 0  -- 拒否
            end
        "#;

        let max_requests = self.config.requests_per_second + self.config.burst;

        let result: Result<i32, RedisError> = redis::Script::new(script)
            .key(redis_key)
            .arg(max_requests)
            .arg(window_size)
            .invoke_async(&mut conn)
            .await;

        match result {
            Ok(val) => {
                debug!("Fixed window rate limit check for {}: {}", key, val);
                Ok(val == 1)
            }
            Err(err) => {
                error!("Failed to execute fixed window rate limit script: {}", err);
                Err(format!(
                    "Failed to execute fixed window rate limit script: {}",
                    err
                ))
            }
        }
    }

    // スライディングウィンドウアルゴリズム
    async fn check_sliding_window(&self, key: &str) -> Result<bool, String> {
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

        let window_size = self.config.window_size as u64;
        let current_window = now / window_size * window_size;
        let previous_window = current_window - window_size;

        let current_key = format!("ratelimit:sliding:{}:{}", key, current_window);
        let previous_key = format!("ratelimit:sliding:{}:{}", key, previous_window);

        // スライディングウィンドウの実装（前回のウィンドウも部分的に考慮）
        let script = r#"
            local current_key = KEYS[1]
            local previous_key = KEYS[2]
            local now = tonumber(ARGV[1])
            local window_size = tonumber(ARGV[2])
            local max_requests = tonumber(ARGV[3])
            local burst = tonumber(ARGV[4])

            -- 現在のウィンドウの開始時間
            local current_window_start = math.floor(now / window_size) * window_size
            -- 経過した割合 (0.0 ~ 1.0)
            local elapsed_ratio = (now - current_window_start) / window_size

            -- 現在のウィンドウのカウントを増加
            local current_count = redis.call('INCR', current_key)
            if current_count == 1 then
                redis.call('EXPIRE', current_key, window_size * 2)
            end

            -- 前回のウィンドウのカウントを取得
            local previous_count = redis.call('GET', previous_key) or "0"
            previous_count = tonumber(previous_count)

            -- 重み付けされたカウント: 現在のカウント + 前回のカウント×(1-経過した割合)
            local weighted_count = current_count + previous_count * (1 - elapsed_ratio)

            -- バーストを含む最大リクエスト数を超えたかチェック
            if weighted_count <= (max_requests + burst) then
                return 1  -- 許可
            else
                return 0  -- 拒否
            end
        "#;

        let result: Result<i32, RedisError> = redis::Script::new(script)
            .key(current_key)
            .key(previous_key)
            .arg(now)
            .arg(window_size)
            .arg(self.config.requests_per_second)
            .arg(self.config.burst)
            .invoke_async(&mut conn)
            .await;

        match result {
            Ok(val) => {
                debug!("Sliding window rate limit check for {}: {}", key, val);
                Ok(val == 1)
            }
            Err(err) => {
                error!(
                    "Failed to execute sliding window rate limit script: {}",
                    err
                );
                Err(format!(
                    "Failed to execute sliding window rate limit script: {}",
                    err
                ))
            }
        }
    }

    // トークンバケットアルゴリズム
    async fn check_token_bucket(&self, key: &str) -> Result<bool, String> {
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

        let redis_key = format!("ratelimit:token:{}", key);
        let refill_time = 1.0 / self.config.requests_per_second as f64; // トークン1つが補充される時間（秒）

        // トークンバケットの実装
        let script = r#"
            local key = KEYS[1]
            local now = tonumber(ARGV[1])
            local refill_time = tonumber(ARGV[2])
            local burst = tonumber(ARGV[3])
            local window_size = tonumber(ARGV[4])

            -- キーが存在するか確認
            local exists = redis.call('EXISTS', key)

            if exists == 0 then
                -- 新規キー: バケットを最大容量で初期化
                redis.call('HSET', key, 'tokens', burst, 'last_refill', now)
                redis.call('EXPIRE', key, window_size * 2)
                return 1 -- 許可
            else
                -- 既存キー: 最後の補充からの経過時間に基づいてトークンを補充
                local tokens = tonumber(redis.call('HGET', key, 'tokens'))
                local last_refill = tonumber(redis.call('HGET', key, 'last_refill'))

                -- 経過時間からトークン補充数を計算
                local elapsed = now - last_refill
                local new_tokens = math.min(burst, tokens + elapsed / refill_time)

                if new_tokens >= 1 then
                    -- トークンが利用可能: トークンを消費
                    redis.call('HSET', key, 'tokens', new_tokens - 1, 'last_refill', now)
                    return 1 -- 許可
                else
                    -- トークンが不足: 補充時間だけ更新
                    redis.call('HSET', key, 'last_refill', now)
                    return 0 -- 拒否
                end
            end
        "#;

        let result: Result<i32, RedisError> = redis::Script::new(script)
            .key(redis_key)
            .arg(now)
            .arg(refill_time)
            .arg(self.config.burst)
            .arg(self.config.window_size)
            .invoke_async(&mut conn)
            .await;

        match result {
            Ok(val) => {
                debug!("Token bucket rate limit check for {}: {}", key, val);
                Ok(val == 1)
            }
            Err(err) => {
                error!("Failed to execute token bucket rate limit script: {}", err);
                Err(format!(
                    "Failed to execute token bucket rate limit script: {}",
                    err
                ))
            }
        }
    }

    // リーキーバケットアルゴリズム
    async fn check_leaky_bucket(&self, key: &str) -> Result<bool, String> {
        let mut conn = match self.get_connection().await {
            Ok(conn) => conn,
            Err(err) => {
                error!("Failed to get Redis connection: {}", err);
                return Err(format!("Failed to get Redis connection: {}", err));
            }
        };

        // 現在のタイムスタンプ（秒）
        let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(n) => n.as_secs() as f64 + n.subsec_micros() as f64 / 1_000_000.0,
            Err(_) => {
                error!("SystemTime before UNIX EPOCH!");
                return Err("SystemTime before UNIX EPOCH!".to_string());
            }
        };

        let redis_key = format!("ratelimit:leaky:{}", key);
        let rate = self.config.requests_per_second as f64; // 1秒あたりの処理レート
        let bucket_size = self.config.burst as f64; // バケットサイズ

        // リーキーバケットの実装
        let script = r#"
            local key = KEYS[1]
            local now = tonumber(ARGV[1])
            local rate = tonumber(ARGV[2])
            local bucket_size = tonumber(ARGV[3])
            local window_size = tonumber(ARGV[4])

            -- キーが存在するか確認
            local exists = redis.call('EXISTS', key)

            if exists == 0 then
                -- 新規キー: レベルを1で初期化、最後のリークタイムを現在に設定
                redis.call('HSET', key, 'level', 1, 'last_leak', now)
                redis.call('EXPIRE', key, window_size * 2)
                return 1 -- 許可
            else
                -- 既存キー: 前回のリークからの経過時間に基づいてバケットをリーク
                local level = tonumber(redis.call('HGET', key, 'level'))
                local last_leak = tonumber(redis.call('HGET', key, 'last_leak'))

                -- 経過時間から減少したレベルを計算
                local elapsed = now - last_leak
                local leaked = rate * elapsed
                local new_level = math.max(0, level - leaked)

                -- 新しいリクエストを追加（水位を上げる）
                new_level = new_level + 1

                if new_level <= bucket_size then
                    -- バケットがオーバーフローしていない: リクエストを許可
                    redis.call('HSET', key, 'level', new_level, 'last_leak', now)
                    return 1 -- 許可
                else
                    -- バケットがオーバーフロー: リクエストを拒否（タイムスタンプだけ更新）
                    redis.call('HSET', key, 'last_leak', now)
                    return 0 -- 拒否
                end
            end
        "#;

        let result: Result<i32, RedisError> = redis::Script::new(script)
            .key(redis_key)
            .arg(now)
            .arg(rate)
            .arg(bucket_size)
            .arg(self.config.window_size)
            .invoke_async(&mut conn)
            .await;

        match result {
            Ok(val) => {
                debug!("Leaky bucket rate limit check for {}: {}", key, val);
                Ok(val == 1)
            }
            Err(err) => {
                error!("Failed to execute leaky bucket rate limit script: {}", err);
                Err(format!(
                    "Failed to execute leaky bucket rate limit script: {}",
                    err
                ))
            }
        }
    }
}
