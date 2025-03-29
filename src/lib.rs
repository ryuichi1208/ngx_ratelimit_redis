
se lazy_static::lazy_static;
use log::{debug, error, info};
use nginx::core::*;
use nginx::http::*;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::Mutex;

mod redis_client;
use redis_client::{RedisRateLimiter, RateLimitConfig};

// モジュールの設定構造体
#[derive(Debug, Clone)]
struct RateLimitRedisConfig {
    redis_url: String,
    rate_limit_key: String, // IPアドレスやAPIキーなどのレート制限キーを特定するための設定
    requests_per_second: u32,
    burst: u32,
    enabled: bool,
}

impl Default for RateLimitRedisConfig {
    fn default() -> Self {
        Self {
            redis_url: "redis://127.0.0.1:6379".to_string(),
            rate_limit_key: "remote_addr".to_string(),
            requests_per_second: 10,
            burst: 5,
            enabled: false,
        }
    }
}

// グローバルなランタイムとRedisクライアントの保持
lazy_static! {
    static ref RUNTIME: Runtime = Runtime::new().expect("Failed to create Tokio runtime");
    static ref REDIS_LIMITER: Arc<Mutex<Option<RedisRateLimiter>>> = Arc::new(Mutex::new(None));
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

// "ratelimit_redis" ディレクティブの設定ハンドラ
#[nginx_handler]
async fn ratelimit_redis_command(
    cf: &mut HttpConfRef,
    cmd: &CommandArgs,
) -> Result<(), String> {
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
        } else {
            return Err(format!("Unknown parameter: {}", arg));
        }
    }

    // コンテキストの更新
    let new_ctx = ModuleContext { config };
    cf.set_module_ctx(new_ctx);

    // Redis接続の初期化
    if enabled {
        let limiter_config = RateLimitConfig {
            redis_url: config.redis_url.clone(),
            requests_per_second: config.requests_per_second,
            burst: config.burst,
        };

        match RUNTIME.block_on(async {
            let mut limiter = REDIS_LIMITER.lock().await;
            *limiter = Some(RedisRateLimiter::new(limiter_config).await?);
            Ok::<(), String>(())
        }) {
            Ok(_) => info!("Redis Rate Limiter initialized"),
            Err(e) => error!("Failed to initialize Redis connection: {}", e),
        }
    }

    Ok(())
}

// リクエストハンドラ
#[nginx_handler]
async fn ratelimit_handler(r: &mut Request) -> Status {
    let ctx = r.get_module_ctx::<ModuleContext>().unwrap_or_else(|| {
        let ctx = ModuleContext {
            config: RateLimitRedisConfig::default(),
        };
        r.set_module_ctx(ctx.clone());
        ctx
    });

    if !ctx.config.enabled {
        return Status::Declined;
    }

    // レート制限キー（例：IPアドレス）の取得
    let key = match ctx.config.rate_limit_key.as_str() {
        "remote_addr" => {
            if let Some(addr) = r.connection().remote_addr() {
                addr.to_string()
            } else {
                error!("Could not get remote address");
                return Status::Declined;
            }
        },
        // カスタムヘッダーやその他のキーに対応する場合
        _ => {
            if ctx.config.rate_limit_key.starts_with("http_") {
                let header_name = ctx.config.rate_limit_key.trim_start_matches("http_");
                if let Some(value) = r.headers_in().get(header_name) {
                    value.to_string()
                } else {
                    error!("Header not found: {}", header_name);
                    return Status::Declined;
                }
            } else {
                ctx.config.rate_limit_key.clone()
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
        r.headers_out().set("X-RateLimit-Limit", &ctx.config.requests_per_second.to_string());
        r.headers_out().set("X-RateLimit-Remaining", "0");
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
    let cmd = HttpCommand::new(ratelimit_redis_command);
    cmcf.register_command("ratelimit_redis", cmd)?;

    Ok(())
}

#[nginx_module_export]
static mut ngx_ratelimit_redis_commands: [Command; 1] = [
    Command::HttpMain(http_preinit),
];

#[nginx_module_init]
static mut NGX_HTTP_MODULE: HttpModule = HttpModule::new(
    module_init,
    module_exit,
    http_init,
    None,
    None,
);
