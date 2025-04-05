use log::{debug, error, info};
use redis::{aio::Connection, AsyncCommands, Client, RedisError};
use serde::{Deserialize, Serialize};
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

/// Redis接続のオプションを設定するための構造体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConnectionOptions {
    /// 接続タイムアウト（ミリ秒）
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout: u64,

    /// コマンド実行タイムアウト（ミリ秒）
    #[serde(default = "default_command_timeout")]
    pub command_timeout: u64,

    /// 接続失敗時のリトライ回数
    #[serde(default = "default_retry_count")]
    pub retry_count: u32,

    /// リトライ間の待機時間（ミリ秒）
    #[serde(default = "default_retry_delay")]
    pub retry_delay: u64,

    /// 認証パスワード（オプション）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    /// 使用するデータベース番号
    #[serde(default = "default_database")]
    pub database: i64,

    /// 接続プールの最大サイズ
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,

    /// クラスタモードを有効にするかどうか
    #[serde(default)]
    pub cluster_mode: bool,

    /// TLS接続を有効にするかどうか
    #[serde(default)]
    pub tls_enabled: bool,

    /// キープアライブ間隔（秒、0の場合は無効）
    #[serde(default)]
    pub keepalive: u64,
}

impl Default for RedisConnectionOptions {
    fn default() -> Self {
        Self {
            connect_timeout: default_connect_timeout(),
            command_timeout: default_command_timeout(),
            retry_count: default_retry_count(),
            retry_delay: default_retry_delay(),
            password: None,
            database: default_database(),
            pool_size: default_pool_size(),
            cluster_mode: false,
            tls_enabled: false,
            keepalive: 0,
        }
    }
}

// デフォルト値関数
fn default_connect_timeout() -> u64 {
    5000 // 5秒
}

fn default_command_timeout() -> u64 {
    2000 // 2秒
}

fn default_retry_count() -> u32 {
    3
}

fn default_retry_delay() -> u64 {
    500 // 500ミリ秒
}

fn default_database() -> i64 {
    0
}

fn default_pool_size() -> u32 {
    10
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub redis_url: String,
    pub requests_per_second: u32,
    pub burst: u32,
    pub algorithm: RateLimitAlgorithm,
    pub window_size: u32, // 秒単位のウィンドウサイズ（固定ウィンドウとスライディングウィンドウ用）
    pub redis_options: RedisConnectionOptions,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            redis_url: "redis://127.0.0.1:6379".to_string(),
            requests_per_second: 10,
            burst: 5,
            algorithm: RateLimitAlgorithm::SlidingWindow,
            window_size: 60, // デフォルトは1分
            redis_options: RedisConnectionOptions::default(),
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

        // 接続オプションをログに出力
        info!("Redis connection options: connect_timeout={}ms, command_timeout={}ms, retry_count={}, database={}",
            config.redis_options.connect_timeout,
            config.redis_options.command_timeout,
            config.redis_options.retry_count,
            config.redis_options.database);

        // カスタム接続オプションを適用したURL構築
        let url_str = if let Some(pwd) = &config.redis_options.password {
            // パスワードがある場合はURLに組み込む
            let mut redis_url = redis::parse_redis_url(&config.redis_url)
                .map_err(|e| format!("Failed to parse Redis URL: {}", e))?;

            // 認証情報を更新
            redis_url.password = Some(pwd.clone());
            redis_url.db = config.redis_options.database;

            redis_url.to_string()
        } else {
            // パスワードがない場合は元のURLを使用
            config.redis_url.clone()
        };

        // Redisクライアントオプションを構築
        let client_builder = redis::Client::build_with_options(redis::ClientOptions {
            url: url_str.clone(),
            ..Default::default()
        });

        // 接続タイムアウトを設定
        let client_builder = if config.redis_options.connect_timeout > 0 {
            client_builder
                .connection_timeout(Duration::from_millis(config.redis_options.connect_timeout))
        } else {
            client_builder
        };

        // キープアライブを設定
        let client_builder = if config.redis_options.keepalive > 0 {
            client_builder.keep_alive(Duration::from_secs(config.redis_options.keepalive))
        } else {
            client_builder
        };

        // クライアントを構築
        let client = match client_builder.build() {
            Ok(client) => client,
            Err(err) => {
                error!("Failed to create Redis client: {}", err);
                return Err(format!("Failed to create Redis client: {}", err));
            }
        };

        // 接続テスト（リトライロジックを使用）
        let mut last_error = None;
        let mut conn = None;

        for attempt in 0..=config.redis_options.retry_count {
            match client.get_async_connection().await {
                Ok(connection) => {
                    conn = Some(connection);
                    break;
                }
                Err(err) => {
                    error!(
                        "Failed to connect to Redis (attempt {}/{}): {}",
                        attempt + 1,
                        config.redis_options.retry_count + 1,
                        err
                    );
                    last_error = Some(err);

                    if attempt < config.redis_options.retry_count {
                        // リトライ前に待機
                        tokio::time::sleep(Duration::from_millis(config.redis_options.retry_delay))
                            .await;
                    }
                }
            }
        }

        // 全てのリトライが失敗した場合
        if conn.is_none() {
            let err_msg = format!(
                "Failed to connect to Redis after {} attempts: {}",
                config.redis_options.retry_count + 1,
                last_error.map_or_else(|| "Unknown error".to_string(), |e| e.to_string())
            );
            error!("{}", err_msg);
            return Err(err_msg);
        }

        // 接続テスト
        let mut conn = conn.unwrap();

        // コマンド実行タイムアウトの設定（メッセージパッシングで実装）
        let ping_timeout = config.redis_options.command_timeout;
        let ping_result = tokio::time::timeout(
            Duration::from_millis(ping_timeout),
            redis::cmd("PING").query_async::<_, String>(&mut conn),
        )
        .await;

        // タイムアウトとエラーを処理
        match ping_result {
            Ok(redis_result) => match redis_result {
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
            },
            Err(_) => {
                error!("Redis PING command timed out after {}ms", ping_timeout);
                return Err(format!(
                    "Redis PING command timed out after {}ms",
                    ping_timeout
                ));
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

        // コマンドタイムアウトの設定
        let command_timeout = self.config.redis_options.command_timeout;
        let script_result = tokio::time::timeout(
            Duration::from_millis(command_timeout),
            redis::Script::new(script)
                .key(redis_key)
                .arg(max_requests)
                .arg(window_size)
                .invoke_async(&mut conn),
        )
        .await;

        match script_result {
            Ok(redis_result) => match redis_result {
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
            },
            Err(_) => {
                error!(
                    "Fixed window rate limit check timed out after {}ms",
                    command_timeout
                );
                Err(format!(
                    "Fixed window rate limit check timed out after {}ms",
                    command_timeout
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

        // コマンドタイムアウトの設定
        let command_timeout = self.config.redis_options.command_timeout;
        let script_result = tokio::time::timeout(
            Duration::from_millis(command_timeout),
            redis::Script::new(script)
                .key(current_key)
                .key(previous_key)
                .arg(now)
                .arg(window_size)
                .arg(self.config.requests_per_second)
                .arg(self.config.burst)
                .invoke_async(&mut conn),
        )
        .await;

        match script_result {
            Ok(redis_result) => match redis_result {
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
            },
            Err(_) => {
                error!(
                    "Sliding window rate limit check timed out after {}ms",
                    command_timeout
                );
                Err(format!(
                    "Sliding window rate limit check timed out after {}ms",
                    command_timeout
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

        // コマンドタイムアウトの設定
        let command_timeout = self.config.redis_options.command_timeout;
        let script_result = tokio::time::timeout(
            Duration::from_millis(command_timeout),
            redis::Script::new(script)
                .key(redis_key)
                .arg(now)
                .arg(refill_time)
                .arg(self.config.burst)
                .arg(self.config.window_size)
                .invoke_async(&mut conn),
        )
        .await;

        match script_result {
            Ok(redis_result) => match redis_result {
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
            },
            Err(_) => {
                error!(
                    "Token bucket rate limit check timed out after {}ms",
                    command_timeout
                );
                Err(format!(
                    "Token bucket rate limit check timed out after {}ms",
                    command_timeout
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

        // コマンドタイムアウトの設定
        let command_timeout = self.config.redis_options.command_timeout;
        let script_result = tokio::time::timeout(
            Duration::from_millis(command_timeout),
            redis::Script::new(script)
                .key(redis_key)
                .arg(now)
                .arg(rate)
                .arg(bucket_size)
                .arg(self.config.window_size)
                .invoke_async(&mut conn),
        )
        .await;

        match script_result {
            Ok(redis_result) => match redis_result {
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
            },
            Err(_) => {
                error!(
                    "Leaky bucket rate limit check timed out after {}ms",
                    command_timeout
                );
                Err(format!(
                    "Leaky bucket rate limit check timed out after {}ms",
                    command_timeout
                ))
            }
        }
    }
}
