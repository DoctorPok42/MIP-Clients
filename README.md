# MIP-Clients

[![npm version](https://img.shields.io/npm/v/@mip-client/ts.svg?label=npm)](https://www.npmjs.com/package/@mip-client/ts)
[![PyPI version](https://img.shields.io/pypi/v/mip-client-python.svg?label=pypi)](https://pypi.org/project/mip-client-python/)
[![Crates.io](https://img.shields.io/crates/v/mip-client.svg?label=crates.io)](https://crates.io/crates/mip-client)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

This repository contains client implementations for the MIP (MSIP) protocol, which is used for real-time communication in various applications. The clients are designed to handle connections, events, errors, and auto-reconnection seamlessly.

**Server implementation:** [MIP Server](https://github.com/DoctorPok42/MIP)

## Clients

- **TypeScript Client**: A robust client implementation in TypeScript, providing type safety and modern JavaScript features. It is suitable for both browser and Node.js environments.

- **Python Client**: An asynchronous client implementation in Python, leveraging asyncio for efficient handling of connections and events. It is ideal for server-side applications or scripts that need to interact with MIP servers.

- **Rust Client**: A high-performance client implementation in Rust, utilizing Tokio for async I/O. It offers type safety and is perfect for applications that require low latency and high throughput.

## Installation

To install the TypeScript client, use npm:

```bash
npm install @mip-client/ts
```

To install the Python client, use pip:

```bash
pip install mip-client-python
```

To install the Rust client, add it to your Cargo.toml:

```toml
[dependencies]
mip-client = "1.0"
```

## Usage

[TypeScript client usage](./mip-client-ts/README.md)  
[Python client usage](./mip-client-python/README.md)  
[Rust client usage](./mip-client-rust/README.md)

## Contributing

Contributions are welcome! Please open an issue or submit a pull request with your improvements.

## License

This project is licensed under the MIT License. See the [LICENSE](./LICENSE) file for details.
