# Deploying seserver behind a Cloudflare Tunnel (Debian)

This deploys the relay on a Debian box and exposes it at `wss://relay.example.com`
through a **Cloudflare Tunnel** (`cloudflared`). Cloudflare terminates TLS at the
edge with its own certificate for your domain — you do **not** manage cert files on
the origin.

## Important: what works through a tunnel

A Cloudflare Tunnel proxies **HTTP/WebSocket (TCP) only — not arbitrary UDP**.

| Feature | Transport | Through the tunnel? |
| --- | --- | --- |
| Relay control channel + message relay | WebSocket (`wss`) | yes |
| STUN discovery | UDP `:3478` | no |
| Direct P2P hole-punching | UDP | no |

So over a tunnel the app runs **relay-only**: every message rides the `wss` channel
and is forwarded by the server. It stays end-to-end encrypted (the relay only moves
ciphertext), but there is no direct peer-to-peer path and no forward secrecy on the
relayed session (see the client README threat model). The client's retry/relay
fallback handles this automatically.

> Want real P2P + STUN? You need the origin reachable over UDP — e.g. a DNS-only
> (grey-cloud) `A` record to the box's public IP with your own TLS (Let's Encrypt or
> a Cloudflare Origin Certificate) and ports `3000/tcp` + `3478/udp` open. That is a
> different setup from the tunnel below.

## 1. DNS

In the Cloudflare dashboard your domain is already added. The tunnel step below
creates the `relay.example.com` record for you — no manual `A`/`CNAME` needed.

## 2. Build the server on the Debian box

```bash
sudo apt-get update && sudo apt-get install -y build-essential pkg-config
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
git clone https://github.com/Lukidere/sechat-server.git
cd sechat-server && cargo build --release   # -> target/release/seserver
sudo install -m755 target/release/seserver /usr/local/bin/seserver
```

## 3. Install cloudflared + create the tunnel

```bash
# install (amd64)
curl -L https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64.deb -o cloudflared.deb
sudo dpkg -i cloudflared.deb

cloudflared tunnel login                       # opens a browser, pick your domain
cloudflared tunnel create sechat               # writes ~/.cloudflared/<UUID>.json
cloudflared tunnel route dns sechat relay.example.com
```

`~/.cloudflared/config.yml`:

```yaml
tunnel: sechat
credentials-file: /root/.cloudflared/<UUID>.json

ingress:
  - hostname: relay.example.com
    service: http://localhost:3000      # plain ws; edge is wss
  - service: http_status:404
```

## 4. systemd units

`/etc/systemd/system/seserver.service` — runs plain `ws://` on localhost (the tunnel
provides TLS, so `SECHAT_DEV_INSECURE=1` is correct **only because** it sits behind
cloudflared on loopback; never expose port 3000 publicly):

```ini
[Unit]
Description=sechat relay
After=network.target

[Service]
Environment=SECHAT_DEV_INSECURE=1
# Environment=SECHAT_DEBUG=1
ExecStart=/usr/local/bin/seserver
WorkingDirectory=/var/lib/seserver
User=seserver
Restart=on-failure
RestartSec=2

[Install]
WantedBy=multi-user.target
```

```bash
sudo useradd --system --home /var/lib/seserver --create-home seserver
sudo systemctl enable --now seserver
sudo cloudflared service install     # runs the tunnel as a systemd service
sudo systemctl enable --now cloudflared
```

Lock the box down — only cloudflared should reach the app:

```bash
sudo ufw allow OpenSSH
sudo ufw deny 3000/tcp
sudo ufw enable
```

## 5. Point the clients at it

Client uses `wss://` by default, which matches the Cloudflare edge:

```bash
# do NOT set SECHAT_DEV_INSECURE on the client — it must use wss to the CF edge
SECHAT_SERVER=relay.example.com sechat-gui
# or in-app: Options -> Server -> relay.example.com
```

STUN is unreachable over the tunnel, so leave `SECHAT_STUN` unset; the client falls
back to relay automatically.

## Verifying

```bash
# from anywhere: the CF edge should speak WebSocket over TLS
curl -sI https://relay.example.com/ws        # 400/426 upgrade-required = reachable
sudo journalctl -u seserver -f               # watch auth + relay logs
sudo journalctl -u cloudflared -f
```

## Persisted state

The server writes its mailbox (offline blobs) to `./mailbox` in its working dir
(`/var/lib/seserver/mailbox`). It is `.gitignore`d; back it up if offline delivery
matters. It only ever holds ciphertext addressed to identity hashes.
