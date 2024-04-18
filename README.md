# Ethereum Node Monitor

## Overview

This tool is designed to monitor an Ethereum node RPC endpoint. It provides real-time tracking of block frequencies and offers an API for further integrations.

## Quick Start

### Prerequisites

* Rust installed on your system
* Cargo (Rust's package manager)
* Access to an Ethereum node (either locally or via an external provider)

### Installation and Running

#### Clone the Repository

Ensure you have git installed on your machine and clone the repository containing the Rust project:

```bash
git clone <repository_url>
cd <project_directory>
```

#### Build the Project

Inside the project directory, compile the code with Cargo:

```bash
cargo build --release
```

#### Run the Monitor

After building, you can run the monitor using:

```bash
cargo run --release -- --rpc_url "http://localhost:8545"
```

Here are some common arguments you can use:

* `--listen`: Specify the listen address for the API. Default is "127.0.0.1:8080".
* `--rpc-url`: JSON-RPC URL of the Ethereum node. Default is "http://localhost:8545".
* `--block-frequency`: Expected block frequency (in seconds) of the Ethereum node. Default is 12.
* `--tracing`: Enable OpenTelemetry tracing. Default is `false`.

Example:

```bash
cargo run --release -- --listen "127.0.0.1:8080" --rpc_url "http://localhost:8545" --block_frequency 12 --tracing false
```

## Advanced Usage

### Enabling Jaeger Tracing Spans

To enable OpenTelemetry tracing with Jaeger, follow these steps:

#### Run Jaeger Container

You'll need Docker installed on your system. Run the following command to start a Jaeger container:

```bash
docker run --rm --name jaeger \
  -e COLLECTOR_ZIPKIN_HOST_PORT=:9411 -e COLLECTOR_OTLP_ENABLED=true \
  -p 6831:6831/udp -p 6832:6832/udp -p 5778:5778 \
  -p 16686:16686 -p 4317:4317 -p 4318:4318 \
  -p 14250:14250 -p 14268:14268 -p 14269:14269 -p 9411:9411 \
  jaegertracing/all-in-one:1.56
```

This command sets up Jaeger to collect traces on various ports and exposes the Jaeger UI on http://localhost:16686.

#### Run the Monitor with Tracing Enabled

Modify the run command to enable tracing:

```bash
cargo run --release -- --rpc_url "http://localhost:8545" --tracing true
```

Ensure that your application is configured to send telemetry data to the Jaeger collector running in Docker.

#### Monitoring the Jaeger UI

After enabling tracing and running your monitor application, you can view the traces by accessing the Jaeger UI:

Open your web browser and navigate to http://localhost:16686.

This UI allows you to view detailed tracing information about the interactions and performance of your Ethereum node monitoring tool.