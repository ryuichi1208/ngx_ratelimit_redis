#!/usr/bin/env node

const http = require('http');

// レート制限カウンター
const rateLimits = {
  ip: {},
  api: {}
};

// リクエストカウントをリセットする間隔（ミリ秒）
const RESET_INTERVAL = 10000; // 10秒

// レート制限の設定
const RATE_LIMITS = {
  '/': { limit: 10 },                // IPベース、10リクエスト/間隔
  '/api': { limit: 5 },              // APIキーベース、5リクエスト/間隔
  '/sliding': { limit: 8 },          // スライディングウィンドウ
  '/fixed': { limit: 8 },            // 固定ウィンドウ
  '/token': { limit: 8 },            // トークンバケット
  '/leaky': { limit: 8 }             // リーキーバケット
};

// レート制限しないパス
const NO_LIMIT_PATHS = ['/static'];

// レート制限をチェックする関数
function checkRateLimit(path, identifier) {
  if (NO_LIMIT_PATHS.some(p => path.startsWith(p))) {
    return {
      limited: false,
      limit: 1000,
      count: 0,
      remaining: 1000,
      reset: 0
    };
  }

  // パスのレート制限設定を取得
  const config = RATE_LIMITS[path] || RATE_LIMITS['/'];

  // 使用するストアを決定（API vs IP）
  const store = path === '/api' ? rateLimits.api : rateLimits.ip;

  // カウンターが存在しない場合は初期化
  if (!store[identifier]) {
    store[identifier] = {
      count: 0,
      lastReset: Date.now()
    };
  }

  // 前回のリセットから時間が経過している場合はリセット
  if (Date.now() - store[identifier].lastReset > RESET_INTERVAL) {
    store[identifier].count = 0;
    store[identifier].lastReset = Date.now();
  }

  // カウンターをインクリメント
  store[identifier].count++;

  // 制限を超えているかチェック
  const limited = store[identifier].count > config.limit;
  const remaining = limited ? 0 : config.limit - store[identifier].count;

  return {
    limited,
    count: store[identifier].count,
    limit: config.limit,
    remaining,
    reset: Math.ceil((store[identifier].lastReset + RESET_INTERVAL - Date.now()) / 1000)
  };
}

// HTTPサーバーの作成
const server = http.createServer((req, res) => {
  const url = new URL(req.url, `http://${req.headers.host}`);
  const path = url.pathname;

  // レート制限のためのキーを取得（APIキーまたはIP）
  let identifier;
  if (path === '/api') {
    identifier = req.headers['x-api-key'] || 'anonymous';
  } else {
    identifier = req.socket.remoteAddress;
  }

  // レート制限をチェック
  const rateLimit = checkRateLimit(path, identifier);

  // レスポンスヘッダーを設定
  res.setHeader('X-RateLimit-Limit', rateLimit.limit);
  res.setHeader('X-RateLimit-Remaining', rateLimit.remaining);
  res.setHeader('X-RateLimit-Reset', rateLimit.reset);

  // レート制限を超えている場合は403を返す
  if (rateLimit.limited) {
    res.statusCode = 403;
    res.setHeader('Content-Type', 'application/json');
    res.end(JSON.stringify({ error: 'Rate limit exceeded' }));
    return;
  }

  // 通常のレスポンス
  res.statusCode = 200;
  res.setHeader('Content-Type', 'application/json');

  // レスポンスの作成
  const response = {
    path,
    identifier,
    rateLimit: {
      limit: rateLimit.limit,
      remaining: rateLimit.remaining,
      reset: rateLimit.reset,
      count: rateLimit.count
    }
  };

  res.end(JSON.stringify(response));
});

// サーバーの起動
const PORT = process.env.PORT || 8080;
server.listen(PORT, () => {
  console.log(`モックNGINXサーバーが起動しました: http://localhost:${PORT}`);
});
