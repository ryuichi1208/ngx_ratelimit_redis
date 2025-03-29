# ngx_ratelimit_redis

[![CI](https://github.com/ryuichi1208/ngx_ratelimit_redis/actions/workflows/ci.yml/badge.svg)](https://github.com/ryuichi1208/ngx_ratelimit_redis/actions/workflows/ci.yml)
[![Docker Build](https://github.com/ryuichi1208/ngx_ratelimit_redis/actions/workflows/docker.yml/badge.svg)](https://github.com/ryuichi1208/ngx_ratelimit_redis/actions/workflows/docker.yml)

A rate limiting module for NGINX using Redis as a backend. Implemented in Rust using [ngx-rust](https://github.com/nginx/ngx-rust).

## Features

- Distributed rate limiting with Redis backend
- Rate limiting based on IP address or custom headers (like API keys)
- Configurable requests per second and burst values
- Multiple rate limiting algorithms:
  - Sliding Window (default)
  - Fixed Window
  - Token Bucket
  - Leaky Bucket

## Building

### Dependencies

- Rust (1.65 or later)
- NGINX (1.22.0 or later recommended)
- Cargo
- Redis (4.0 or later)

### Compilation

```bash
# Specify NGINX version via environment variable
NGX_VERSION=1.26.3 cargo build --release
```

After the build completes, you'll find `target/release/libngx_ratelimit_redis.so` (Linux) or `target/release/libngx_ratelimit_redis.dylib` (MacOS).

### Building with Docker

```bash
# Build Docker image
docker build -t ngx-ratelimit-redis .

# Run container
docker run -d -p 8080:8080 ngx-ratelimit-redis
```

## Installation

Copy the generated module file to your NGINX modules directory:

```bash
# For Linux
sudo cp target/release/libngx_ratelimit_redis.so /usr/lib/nginx/modules/

# For MacOS
sudo cp target/release/libngx_ratelimit_redis.dylib /usr/local/opt/nginx/modules/
```

## Configuration

Load the module in your NGINX configuration file and configure it:

```nginx
# Load the module
load_module modules/libngx_ratelimit_redis.so;

http {
    server {
        # ...

        location / {
            # Enable the module with options
            ratelimit_redis on redis_url=redis://127.0.0.1:6379 key=remote_addr rate=10 burst=5;

            # Other directives...
        }
    }
}
```

### Configuration Options

| Option       | Description                              | Default Value           |
|--------------|------------------------------------------|-------------------------|
| on/off       | Enable/disable the module                | off                     |
| redis_url    | Redis server connection URL              | redis://127.0.0.1:6379  |
| key          | Key used for rate limiting               | remote_addr             |
| rate         | Maximum requests per second              | 10                      |
| burst        | Temporarily allowed excess requests      | 5                       |
| algorithm    | Rate limiting algorithm                  | sliding_window          |
| window_size  | Time window size in seconds              | 60                      |

### Key Types

- `remote_addr`: Client IP address
- `http_[header_name]`: Value of specified HTTP header (e.g., `http_x_api_key`)

### Rate Limiting Algorithms

The module supports the following rate limiting algorithms:

1. **Sliding Window** (`sliding_window`): Provides a smooth rate limiting by considering both current window and previous window with weights. Good balance between accuracy and performance.

2. **Fixed Window** (`fixed_window`): Simplest algorithm that limits requests within fixed time intervals. Efficient but can allow traffic spikes at window boundaries.

3. **Token Bucket** (`token_bucket`): Tokens are added to a bucket at a fixed rate. Each request consumes a token. Allows bursts of traffic while maintaining a long-term rate limit.

4. **Leaky Bucket** (`leaky_bucket`): Processes requests at a constant rate, effectively smoothing out bursty traffic.

## Usage Examples

```nginx
# Default algorithm (Sliding Window)
location / {
    ratelimit_redis on redis_url=redis://127.0.0.1:6379 key=remote_addr rate=10 burst=5;
    # ...
}

# Fixed Window algorithm
location /fixed {
    ratelimit_redis on redis_url=redis://127.0.0.1:6379 key=remote_addr rate=10 burst=5 algorithm=fixed_window window_size=60;
    # ...
}

# Token Bucket algorithm
location /token {
    ratelimit_redis on redis_url=redis://127.0.0.1:6379 key=remote_addr rate=10 burst=20 algorithm=token_bucket;
    # ...
}

# Leaky Bucket algorithm
location /leaky {
    ratelimit_redis on redis_url=redis://127.0.0.1:6379 key=remote_addr rate=5 burst=10 algorithm=leaky_bucket;
    # ...
}

# API key-based rate limiting
location /api {
    ratelimit_redis on redis_url=redis://redis-server:6379 key=http_x_api_key rate=5 burst=2;
    # ...
}

# Disable rate limiting
location /static {
    ratelimit_redis off;
    # ...
}
```

## Testing

Test scripts are available in the `script` directory to verify the functionality of this module.

### Basic Testing

```bash
# Basic rate limit testing
./script/test_rate_limit.sh

# Testing with custom settings
./script/test_rate_limit.sh -n 30 -w 0.2
```

### Testing with Docker

```bash
# Docker testing (builds image, starts container, runs tests)
./script/docker_test.sh

# Keep container running after tests
./script/docker_test.sh --keep
```

### Benchmarking

```bash
# Benchmark rate limiting functionality
./script/benchmark_rate_limit.sh

# Benchmark with API key
./script/benchmark_rate_limit.sh --api -e /api
```

For detailed testing instructions, see [script/README.md](script/README.md).

## How It Works

This module implements rate limiting using Redis as a backend with various algorithms for different use cases. This enables accurate rate limiting even in distributed environments.

1. Extract the key (IP address, API key, etc.) when a request arrives
2. Apply the selected rate limiting algorithm with Redis for distributed state
3. Return 403 Forbidden if the configured limit is exceeded
4. Continue request processing if within limits

When a client is rate limited, the module returns the following headers:
- `X-RateLimit-Limit`: Maximum requests per second
- `X-RateLimit-Remaining`: Remaining requests (0 when limited)
- `X-RateLimit-Algorithm`: The algorithm used for rate limiting

## License

Apache License 2.0

## References

- [NGINX](https://nginx.org/)
- [ngx-rust](https://github.com/nginx/ngx-rust)
- [Redis](https://redis.io/)
