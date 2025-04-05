use lazy_static::lazy_static;
use log::{debug, error, info};
use ngx::core::*;
use ngx::http::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::Mutex;

mod config;
mod redis_client;

use config::{ConfigFile, RateLimitSettings};
use redis_client::{RateLimitAlgorithm, RateLimitConfig, RedisConnectionOptions, RedisRateLimiter};

// モジュールの設定構造体
#[derive(Debug, Clone)]
struct RateLimitRedisConfig {
    redis_url: String,
    rate_limit_key: String, // IPアドレスやAPIキーなどのレート制限キーを特定するための設定
    requests_per_second: u32,
    burst: u32,
    enabled: bool,
    algorithm: RateLimitAlgorithm,
    window_size: u32,
    config_file_path: Option<String>,
    redis_options: RedisConnectionOptions,
}

impl Default for RateLimitRedisConfig {
    fn default() -> Self {
        Self {
            redis_url: "redis://127.0.0.1:6379".to_string(),
            rate_limit_key: "remote_addr".to_string(),
            requests_per_second: 10,
            burst: 5,
            enabled: false,
            algorithm: RateLimitAlgorithm::SlidingWindow,
            window_size: 60,
            config_file_path: None,
            redis_options: RedisConnectionOptions::default(),
        }
    }
}

// グローバルなランタイム、Redisクライアント、設定ファイルの保持
lazy_static! {
    static ref RUNTIME: Runtime = Runtime::new().expect("Failed to create Tokio runtime");
    static ref REDIS_LIMITER: Arc<Mutex<Option<RedisRateLimiter>>> = Arc::new(Mutex::new(None));
    static ref CONFIG_FILE: Arc<Mutex<Option<ConfigFile>>> = Arc::new(Mutex::new(None));
    static ref LOCATION_SETTINGS: Arc<Mutex<HashMap<String, RateLimitRedisConfig>>> =
        Arc::new(Mutex::new(HashMap::new()));
}

// モジュールのコンテキスト管理
#[derive(Clone)]
struct ModuleContext {
    config: RateLimitRedisConfig,
}

// モジュール定義
nginx_module!(ngx_ratelimit_redis_module);

// モジュールの初期化関数
#[nginx_handler]
async fn module_init(cf: &mut MainConf) -> Result<(), String> {
    info!("Initializing Redis Rate Limiter module");
    Ok(())
}

// モジュールの終了関数
#[nginx_handler]
async fn module_exit() -> Result<(), String> {
    info!("Shutting down Redis Rate Limiter module");
    Ok(())
}

// HTTP部分の初期化
#[nginx_handler]
async fn http_init(cmcf: &mut HttpMainConf) -> Result<(), String> {
    let handler_loc = HttpLocationHandler::new(ratelimit_handler);
    let _ = cmcf.register_loc_handler("ratelimit_redis", handler_loc);

    Ok(())
}

// 設定ファイルの読み込み
async fn load_config_file(path: &str) -> Result<ConfigFile, String> {
    match ConfigFile::from_file(path) {
        Ok(config) => {
            info!("Successfully loaded configuration from {}", path);
            Ok(config)
        }
        Err(e) => {
            error!("Failed to load configuration file: {}", e);
            Err(e)
        }
    }
}

// 設定ファイルから特定のLocationの設定を取得して適用
fn apply_config_from_file(config_file: &ConfigFile, location: &str) -> RateLimitRedisConfig {
    let settings = config_file.get_settings(location);
    apply_settings_to_config(settings)
}

// RateLimitSettingsからRateLimitRedisConfigを生成
fn apply_settings_to_config(settings: RateLimitSettings) -> RateLimitRedisConfig {
    let algorithm = ConfigFile::parse_algorithm(&settings.algorithm)
        .unwrap_or(RateLimitAlgorithm::SlidingWindow);

    RateLimitRedisConfig {
        redis_url: settings.redis_url,
        rate_limit_key: settings.key,
        requests_per_second: settings.rate,
        burst: settings.burst,
        enabled: settings.enabled,
        algorithm,
        window_size: settings.window_size,
        config_file_path: None,
        redis_options: settings.redis_options,
    }
}

// "ratelimit_redis_config" ディレクティブの設定ハンドラ
#[nginx_handler]
async fn ratelimit_redis_config_command(
    cf: &mut HttpConfRef,
    cmd: &CommandArgs,
) -> Result<(), String> {
    let args = cmd.args();
    if args.len() != 1 {
        return Err("Syntax: ratelimit_redis_config /path/to/config.json".to_string());
    }

    let config_path = args[0].as_str().to_string();
    info!("Loading rate limit configuration from {}", config_path);

    // 設定ファイルを読み込む
    let config_file = match RUNTIME.block_on(load_config_file(&config_path)) {
        Ok(config) => config,
        Err(e) => return Err(format!("Failed to load config file: {}", e)),
    };

    // グローバル設定に保存
    let mut global_config = CONFIG_FILE.lock().await;
    *global_config = Some(config_file);

    // デフォルト設定を取得
    if let Some(config) = &*global_config {
        // Redisの初期化
        if config.default.enabled {
            let limiter_config = RateLimitConfig {
                redis_url: config.default.redis_url.clone(),
                requests_per_second: config.default.rate,
                burst: config.default.burst,
                algorithm: ConfigFile::parse_algorithm(&config.default.algorithm)
                    .unwrap_or(RateLimitAlgorithm::SlidingWindow),
                window_size: config.default.window_size,
                redis_options: config.default.redis_options.clone(),
            };

            match RUNTIME.block_on(async {
                let mut limiter = REDIS_LIMITER.lock().await;
                *limiter = Some(RedisRateLimiter::new(limiter_config).await?);
                Ok::<(), String>(())
            }) {
                Ok(_) => info!("Redis Rate Limiter initialized from config file"),
                Err(e) => error!("Failed to initialize Redis connection: {}", e),
            }
        }
    }

    Ok(())
}

// Redis接続オプションを解析する
fn parse_redis_option(arg: &str, config: &mut RateLimitRedisConfig) -> Result<(), String> {
    if arg.starts_with("redis_connect_timeout=") {
        let timeout_str = arg.trim_start_matches("redis_connect_timeout=");
        if let Ok(timeout) = timeout_str.parse::<u64>() {
            config.redis_options.connect_timeout = timeout;
        } else {
            return Err(format!(
                "Invalid redis_connect_timeout value: {}",
                timeout_str
            ));
        }
    } else if arg.starts_with("redis_command_timeout=") {
        let timeout_str = arg.trim_start_matches("redis_command_timeout=");
        if let Ok(timeout) = timeout_str.parse::<u64>() {
            config.redis_options.command_timeout = timeout;
        } else {
            return Err(format!(
                "Invalid redis_command_timeout value: {}",
                timeout_str
            ));
        }
    } else if arg.starts_with("redis_retry_count=") {
        let retry_str = arg.trim_start_matches("redis_retry_count=");
        if let Ok(retry) = retry_str.parse::<u32>() {
            config.redis_options.retry_count = retry;
        } else {
            return Err(format!("Invalid redis_retry_count value: {}", retry_str));
        }
    } else if arg.starts_with("redis_retry_delay=") {
        let delay_str = arg.trim_start_matches("redis_retry_delay=");
        if let Ok(delay) = delay_str.parse::<u64>() {
            config.redis_options.retry_delay = delay;
        } else {
            return Err(format!("Invalid redis_retry_delay value: {}", delay_str));
        }
    } else if arg.starts_with("redis_password=") {
        let password = arg.trim_start_matches("redis_password=").to_string();
        if !password.is_empty() {
            config.redis_options.password = Some(password);
        }
    } else if arg.starts_with("redis_database=") {
        let db_str = arg.trim_start_matches("redis_database=");
        if let Ok(db) = db_str.parse::<i64>() {
            config.redis_options.database = db;
        } else {
            return Err(format!("Invalid redis_database value: {}", db_str));
        }
    } else if arg.starts_with("redis_pool_size=") {
        let pool_str = arg.trim_start_matches("redis_pool_size=");
        if let Ok(pool) = pool_str.parse::<u32>() {
            config.redis_options.pool_size = pool;
        } else {
            return Err(format!("Invalid redis_pool_size value: {}", pool_str));
        }
    } else if arg.starts_with("redis_cluster_mode=") {
        let mode_str = arg.trim_start_matches("redis_cluster_mode=");
        if mode_str == "on" {
            config.redis_options.cluster_mode = true;
        } else if mode_str == "off" {
            config.redis_options.cluster_mode = false;
        } else {
            return Err(format!("Invalid redis_cluster_mode value: {}", mode_str));
        }
    } else if arg.starts_with("redis_tls=") {
        let tls_str = arg.trim_start_matches("redis_tls=");
        if tls_str == "on" {
            config.redis_options.tls_enabled = true;
        } else if tls_str == "off" {
            config.redis_options.tls_enabled = false;
        } else {
            return Err(format!("Invalid redis_tls value: {}", tls_str));
        }
    } else if arg.starts_with("redis_keepalive=") {
        let keepalive_str = arg.trim_start_matches("redis_keepalive=");
        if let Ok(keepalive) = keepalive_str.parse::<u64>() {
            config.redis_options.keepalive = keepalive;
        } else {
            return Err(format!("Invalid redis_keepalive value: {}", keepalive_str));
        }
    } else {
        return Err(format!("Unknown Redis connection option: {}", arg));
    }

    Ok(())
}

// "ratelimit_redis" ディレクティブの設定ハンドラ
#[nginx_handler]
async fn ratelimit_redis_command(cf: &mut HttpConfRef, cmd: &CommandArgs) -> Result<(), String> {
    let ctx = cf.get_module_ctx::<ModuleContext>().unwrap_or_else(|| {
        let ctx = ModuleContext {
            config: RateLimitRedisConfig::default(),
        };
        cf.set_module_ctx(ctx.clone());
        ctx
    });

    let mut config = ctx.config.clone();

    // コマンド引数の解析
    let args = cmd.args();
    if args.len() < 1 {
        return Err("Invalid number of arguments for ratelimit_redis directive".to_string());
    }

    // 有効/無効の設定
    let enabled = match args[0].as_str() {
        "on" => true,
        "off" => false,
        _ => return Err("ratelimit_redis should be 'on' or 'off'".to_string()),
    };

    config.enabled = enabled;

    // オプションのパラメータ解析
    for i in 1..args.len() {
        let arg = args[i].as_str();

        if arg.starts_with("redis_url=") {
            config.redis_url = arg.trim_start_matches("redis_url=").to_string();
        } else if arg.starts_with("key=") {
            config.rate_limit_key = arg.trim_start_matches("key=").to_string();
        } else if arg.starts_with("rate=") {
            let rate_str = arg.trim_start_matches("rate=");
            if let Ok(rate) = rate_str.parse::<u32>() {
                config.requests_per_second = rate;
            } else {
                return Err(format!("Invalid rate value: {}", rate_str));
            }
        } else if arg.starts_with("burst=") {
            let burst_str = arg.trim_start_matches("burst=");
            if let Ok(burst) = burst_str.parse::<u32>() {
                config.burst = burst;
            } else {
                return Err(format!("Invalid burst value: {}", burst_str));
            }
        } else if arg.starts_with("algorithm=") {
            let algorithm_str = arg.trim_start_matches("algorithm=");
            match RateLimitAlgorithm::from_str(algorithm_str) {
                Ok(algorithm) => config.algorithm = algorithm,
                Err(err) => return Err(err),
            }
        } else if arg.starts_with("window_size=") {
            let window_str = arg.trim_start_matches("window_size=");
            if let Ok(window) = window_str.parse::<u32>() {
                config.window_size = window;
            } else {
                return Err(format!("Invalid window_size value: {}", window_str));
            }
        } else if arg.starts_with("config_file=") {
            let file_path = arg.trim_start_matches("config_file=").to_string();
            config.config_file_path = Some(file_path);
        } else if arg.starts_with("redis_") {
            // Redis接続オプションを解析
            parse_redis_option(arg, &mut config)?;
        } else {
            return Err(format!("Unknown parameter: {}", arg));
        }
    }

    // config_file指定がある場合は設定ファイルを読み込む
    if let Some(file_path) = &config.config_file_path {
        let config_file = match RUNTIME.block_on(load_config_file(file_path)) {
            Ok(cfg) => cfg,
            Err(e) => return Err(format!("Failed to load config file: {}", e)),
        };

        // 現在のロケーションの設定を適用
        let location = cf.loc_conf_get_path().to_string();
        let location_config = apply_config_from_file(&config_file, &location);

        // 設定をマージ
        config.redis_url = location_config.redis_url;
        config.rate_limit_key = location_config.rate_limit_key;
        config.requests_per_second = location_config.requests_per_second;
        config.burst = location_config.burst;
        config.algorithm = location_config.algorithm;
        config.window_size = location_config.window_size;
        config.redis_options = location_config.redis_options;

        // enabledはコマンドラインの設定を優先
        if enabled {
            config.enabled = location_config.enabled;
        }

        // グローバル設定として保存
        let mut global_config = CONFIG_FILE.lock().await;
        *global_config = Some(config_file);

        // ロケーション固有の設定を保存
        let mut location_settings = LOCATION_SETTINGS.lock().await;
        location_settings.insert(location.clone(), config.clone());
    }

    // コンテキストの更新
    let new_ctx = ModuleContext { config };
    cf.set_module_ctx(new_ctx);

    // Redis接続の初期化
    if config.enabled {
        let limiter_config = RateLimitConfig {
            redis_url: config.redis_url.clone(),
            requests_per_second: config.requests_per_second,
            burst: config.burst,
            algorithm: config.algorithm,
            window_size: config.window_size,
            redis_options: config.redis_options,
        };

        match RUNTIME.block_on(async {
            let mut limiter = REDIS_LIMITER.lock().await;
            *limiter = Some(RedisRateLimiter::new(limiter_config).await?);
            Ok::<(), String>(())
        }) {
            Ok(_) => {
                info!(
                    "Redis Rate Limiter initialized with algorithm: {}",
                    config.algorithm
                );
                info!("Redis connection options: connect_timeout={}ms, command_timeout={}ms, retry_count={}, database={}",
                    config.redis_options.connect_timeout,
                    config.redis_options.command_timeout,
                    config.redis_options.retry_count,
                    config.redis_options.database);
            }
            Err(e) => error!("Failed to initialize Redis connection: {}", e),
        }
    }

    Ok(())
}

// リクエストハンドラ
#[nginx_handler]
async fn ratelimit_handler(r: &mut Request) -> Status {
    // 現在のリクエストのロケーションパスを取得
    let location_path = r.get_location_path().to_string();

    // ロケーション固有の設定を確認
    let mut config = {
        let location_settings = LOCATION_SETTINGS.lock().await;
        if let Some(cfg) = location_settings.get(&location_path) {
            cfg.clone()
        } else {
            // グローバルな設定から該当するロケーションの設定を探す
            let global_config = CONFIG_FILE.lock().await;
            if let Some(cfg) = &*global_config {
                apply_config_from_file(cfg, &location_path)
            } else {
                // Context から設定を取得
                let ctx = r.get_module_ctx::<ModuleContext>().unwrap_or_else(|| {
                    let ctx = ModuleContext {
                        config: RateLimitRedisConfig::default(),
                    };
                    r.set_module_ctx(ctx.clone());
                    ctx
                });
                ctx.config.clone()
            }
        }
    };

    if !config.enabled {
        return Status::Declined;
    }

    // レート制限キー（例：IPアドレス）の取得
    let key = match config.rate_limit_key.as_str() {
        "remote_addr" => {
            if let Some(addr) = r.connection().remote_addr() {
                addr.to_string()
            } else {
                error!("Could not get remote address");
                return Status::Declined;
            }
        }
        // カスタムヘッダーやその他のキーに対応する場合
        _ => {
            if config.rate_limit_key.starts_with("http_") {
                let header_name = config.rate_limit_key.trim_start_matches("http_");
                if let Some(value) = r.headers_in().get(header_name) {
                    value.to_string()
                } else {
                    error!("Header not found: {}", header_name);
                    return Status::Declined;
                }
            } else {
                config.rate_limit_key.clone()
            }
        }
    };

    // Redisを使用したレート制限チェック
    let allowed = match RUNTIME.block_on(async {
        let limiter = REDIS_LIMITER.lock().await;
        if let Some(limiter) = &*limiter {
            limiter.check_rate_limit(&key).await
        } else {
            error!("Redis Rate Limiter not initialized");
            Ok(true) // 初期化されていない場合は許可
        }
    }) {
        Ok(allowed) => allowed,
        Err(e) => {
            error!("Rate limit check failed: {}", e);
            true // エラー時は許可（フォールバック）
        }
    };

    if !allowed {
        r.set_status(Status::Forbidden);
        r.headers_out()
            .set("X-RateLimit-Limit", &config.requests_per_second.to_string());
        r.headers_out().set("X-RateLimit-Remaining", "0");
        r.headers_out()
            .set("X-RateLimit-Algorithm", &config.algorithm.to_string());
        r.headers_out().set("Content-Type", "application/json");

        let body = r#"{"error": "rate limit exceeded"}"#;
        r.write_body(body.as_bytes());

        return Status::Done;
    }

    Status::Declined
}

// モジュールコマンドの登録
#[nginx_handler]
async fn http_preinit(cmcf: &mut HttpMainConf) -> Result<(), String> {
    let ratelimit_cmd = HttpCommand::new(ratelimit_redis_command);
    cmcf.register_command("ratelimit_redis", ratelimit_cmd)?;

    let config_cmd = HttpCommand::new(ratelimit_redis_config_command);
    cmcf.register_command("ratelimit_redis_config", config_cmd)?;

    Ok(())
}

#[nginx_module_export]
static mut ngx_ratelimit_redis_commands: [Command; 1] = [Command::HttpMain(http_preinit)];

#[nginx_module_init]
static mut NGX_HTTP_MODULE: HttpModule =
    HttpModule::new(module_init, module_exit, http_init, None, None);
