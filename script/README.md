# Test Scripts

This directory contains scripts for testing the ngx_ratelimit_redis module.

## Available Scripts

### test_rate_limit.sh

A script for testing basic rate limiting functionality. Uses curl to send a specified number of requests and verifies rate limiting behavior.

```bash
./script/test_rate_limit.sh [options]
```

#### Options:
- `-h, --host` - Hostname or IP address (default: localhost)
- `-p, --port` - Port number (default: 8080)
- `-n, --requests` - Number of requests to send (default: 15)
- `-w, --wait` - Wait time between requests in seconds (default: 0.1)
- `-k, --key` - API key (default: test-api-key)

#### Examples:
```bash
# Run test with default settings
./script/test_rate_limit.sh

# Run test with custom settings
./script/test_rate_limit.sh -h 192.168.1.10 -p 80 -n 30 -w 0.5 -k my-api-key
```

### benchmark_rate_limit.sh

A script for benchmarking rate limiting functionality using tools like Apache Bench (ab) and hey.

```bash
./script/benchmark_rate_limit.sh [options]
```

#### Options:
- `-h, --host` - Hostname or IP address (default: localhost)
- `-p, --port` - Port number (default: 8080)
- `-c, --concurrency` - Number of concurrent connections (default: 10)
- `-n, --requests` - Total number of requests (default: 100)
- `-k, --key` - API key (default: test-api-key)
- `-e, --endpoint` - Target endpoint (default: /)
- `--api` - Use API key header

#### Examples:
```bash
# Run benchmark with default settings
./script/benchmark_rate_limit.sh

# Run benchmark with API key
./script/benchmark_rate_limit.sh --api -e /api -n 500 -c 20
```

### docker_test.sh

A script for building and testing the module using Docker. It builds a Docker image, starts NGINX in a container, and runs the test_rate_limit.sh script.

```bash
./script/docker_test.sh [options]
```

#### Options:
- `--keep` - Keep the container running after tests

#### Examples:
```bash
# Run Docker test (container will be stopped after tests)
./script/docker_test.sh

# Keep container running after tests
./script/docker_test.sh --keep
```

## Prerequisites

- Test scripts: curl must be installed
- Benchmark script: Apache Bench (ab) must be installed
- Docker test script: Docker must be installed

## Notes

- During rate limit testing, it's normal for many requests to fail with 403 Forbidden status.
- Benchmark results should be treated as reference values since they don't represent real traffic patterns.
- When running Docker tests, make sure port 8080 is not already in use.
