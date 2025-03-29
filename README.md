# ngx_ratelimit_redis

A rate limiting module for NGINX using Redis as a backend. Implemented in Rust using [ngx-rust](https://github.com/nginx/ngx-rust).

## Features

- Distributed rate limiting with Redis backend
- Rate limiting based on IP address or custom headers (like API keys)
- Configurable requests per second and burst values
- Accurate rate limiting using sliding window algorithm

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

| Option    | Description                                | Default Value           |
|-----------|--------------------------------------------|-------------------------|
| on/off    | Enable/disable the module                  | off                     |
| redis_url | Redis server connection URL                | redis://127.0.0.1:6379  |
| key       | Key used for rate limiting                 | remote_addr             |
| rate      | Maximum requests per second                | 10                      |
| burst     | Temporarily allowed excess requests        | 5                       |

### Key Types

- `remote_addr`: Client IP address
- `http_[header_name]`: Value of specified HTTP header (e.g., `http_x_api_key`)

## Usage Examples

```nginx
# IP-based rate limiting
location / {
    ratelimit_redis on redis_url=redis://127.0.0.1:6379 key=remote_addr rate=10 burst=5;
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

This module implements rate limiting using Redis as a backend with a sliding window algorithm. This enables accurate rate limiting even in distributed environments.

1. Extract the key (IP address, API key, etc.) when a request arrives
2. Increment the counter for that key in Redis
3. Return 403 Forbidden if the configured limit is exceeded
4. Continue request processing if within limits

## License

Apache License 2.0

## References

- [NGINX](https://nginx.org/)
- [ngx-rust](https://github.com/nginx/ngx-rust)
- [Redis](https://redis.io/)
