# seserver

[![CI](https://github.com/dhmztr/sechat-server/actions/workflows/ci.yml/badge.svg)](https://github.com/dhmztr/sechat-server/actions/workflows/ci.yml)

Relay server for [sechat](https://github.com/Lukidere/sechat-client): presence
signaling, an offline-message mailbox, a UDP STUN responder, and TURN-style
relaying of P2P connections. It only ever moves ciphertext addressed to identity
hashes, never plaintext.

## Running

```bash
# production (wss:// — requires a TLS cert/key)
TLS_CERT=./cert.pem TLS_KEY=./key.pem cargo run

# local dev (plain ws://, no TLS — must match the client's SECHAT_DEV_INSECURE)
SECHAT_DEV_INSECURE=1 cargo run
```

| Variable               | Meaning                                         |
| ---------------------- | ----------------------------------------------- |
| `TLS_CERT` / `TLS_KEY` | PEM cert chain / private key (required for wss) |
| `SECHAT_DEV_INSECURE`  | Serve plain `ws://` instead of `wss://`         |
| `STUN_PORT`            | UDP STUN responder port (default `3478`)        |
| `SECHAT_DEBUG`         | Verbose `tracing` output                        |

Listens on `0.0.0.0:3000` (WebSocket) and `0.0.0.0:$STUN_PORT` (UDP).

## Testing

```bash
cargo test
```

See the [client repo](https://github.com/Lukidere/sechat-client) for protocol
details, threat model, and architecture diagrams.
