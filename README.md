# ns

A terminal network monitor for Linux. Shows real-time interface throughput, open listeners, and active TCP connections split by direction (outgoing / incoming). Built with [ratatui](https://github.com/ratatui-org/ratatui).

## Features

- Per-interface RX/TX rate with sparkline history
- Listener table: port, protocol, process, user
- Outgoing connections: remote address, traffic rate, process
- Incoming connections: same columns, filtered by direction
- Interface selector with tab navigation
- Kill a process from the listener or connection view (SIGTERM)

## Requirements

- Linux (reads `/proc/net`, `/proc/<pid>/fd`, `/proc/net/fib_trie`, etc.)
- Rust toolchain (stable)

## Installation

### Quick Install (Pre-compiled)

Download and install the latest release with a single command:

```sh
curl -sSL https://raw.githubusercontent.com/sammwyy/ns/main/scripts/install.sh | sudo bash
```

### Install from Source

Clone the repository and build the project using the provided scripts (requires Rust and UPX):

```sh
git clone https://github.com/sammwyy/ns.git
cd ns
./scripts/build.sh
sudo ./scripts/install.sh
```

Running as a regular user is enough for most data. Process names and PIDs for sockets owned by other users require root.

## Usage

```
ns
```

### Keys

| Key       | Action              |
|-----------|---------------------|
| `Tab`     | Cycle view          |
| `←` / `→` | Switch interface    |
| `↑` / `↓` | Scroll list         |
| `Enter`   | Kill selected process |
| `q`       | Quit                |
| `Ctrl+C`  | Quit                |

### Views

- **Load** — RX and TX sparklines for the selected interface
- **Listeners** — open TCP/UDP ports
- **Outgoing** — established TCP connections initiated by this host
- **Incoming** — established TCP connections accepted by this host

## License

GPL-3.0
