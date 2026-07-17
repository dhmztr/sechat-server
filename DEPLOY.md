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

## 3. Create the Cloudflare Tunnel (one command)

From a checkout of this repo on the box:

```bash
sudo bash deploy/tunnel.sh relay.example.com      # your hostname
```

It installs `cloudflared`, walks you through the one browser-login step, creates
the `sechat` tunnel, adds the DNS record, writes `/etc/cloudflared/config.yml`
(ingress -> `http://localhost:3000`), and runs it as a systemd service. After this,
`https://relay.example.com` reaches the relay with Cloudflare's edge TLS.

<details><summary>What it does manually (if you prefer)</summary>

```bash
curl -L https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64.deb -o cloudflared.deb
sudo dpkg -i cloudflared.deb
cloudflared tunnel login
cloudflared tunnel create sechat
cloudflared tunnel route dns sechat relay.example.com
# then /etc/cloudflared/config.yml per deploy/cloudflared.config.example.yml, and:
sudo cloudflared service install && sudo systemctl enable --now cloudflared
```
</details>

## 4. One-time server setup

The service account, systemd unit (`deploy/seserver.service`), env file
(`/etc/seserver.env`, from `deploy/seserver.env.example`) and the CI sudoers rule
are all installed by one script. On the box, from a checkout of this repo:

```bash
git clone https://github.com/Lukidere/sechat-server.git
cd sechat-server
sudo bash deploy/setup.sh <deploy-ssh-user>   # e.g. your normal SSH user
```

The default env runs plain `ws://` on localhost behind the tunnel (edge does TLS).
Edit `/etc/seserver.env` to switch to direct `wss` with your own cert. The service
starts on the first CI deploy (once `/usr/local/bin/seserver` exists); the tunnel is
already running from step 3.

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
back to relay automatically. To get real direct P2P, see below.

## Direct P2P (true UDP)

A Cloudflare Tunnel can't carry UDP, so behind it the app is relay-only. Real
peer-to-peer needs two UDP things, and only **one** of them touches your server:

1. **STUN** — each client asks your server's UDP STUN responder for its own public
   address. Needs the server reachable on UDP `3478`.
2. **Hole punching** — clients then send UDP straight to *each other*. This never
   touches your server; it only needs the relay to broker the punch, which already
   works over `wss`.

So you don't tunnel UDP — you just make the **STUN port reachable directly**.

### Hybrid: relay via the tunnel, STUN direct (keeps the relay IP hidden)

1. In Cloudflare DNS add a **DNS-only (grey-cloud)** `A` record, e.g.
   `stun.example.com -> <vps-public-ip>`. Grey cloud = not proxied, so UDP reaches
   the box.
2. Open the STUN port on the box (default `3478`, from `/etc/seserver.env`):
   ```bash
   sudo ufw allow 3478/udp
   ```
3. Point clients' STUN at it (the relay still rides the tunnel):
   ```bash
   SECHAT_SERVER=relay.example.com  SECHAT_STUN=stun.example.com:3478  sechat-gui
   ```

Now cone-NAT peers open a direct UDP path; only symmetric-NAT-on-both-ends pairs fall
back to the relay (that is inherent to NAT — the relay is the TURN fallback).

### Fully direct (no tunnel)

Skip the tunnel entirely: DNS-only `A` record to the box, open `3000/tcp` +
`3478/udp`, and give the relay a real cert so clients use `wss://` straight to the
box. In `/etc/seserver.env` set `TLS_CERT`/`TLS_KEY` (Let's Encrypt or a Cloudflare
Origin Certificate) and remove `SECHAT_DEV_INSECURE`. Trade-off: the box's IP is
exposed.

### Notes
- Direct P2P still can't beat symmetric NAT on both ends — those pairs always relay.
- Cloudflare **Spectrum** does proxy UDP, but it is a paid add-on; the grey-cloud STUN
  record above is free.

## Verifying

```bash
# from anywhere: the CF edge should speak WebSocket over TLS
curl -sI https://relay.example.com/ws        # 400/426 upgrade-required = reachable
sudo journalctl -u seserver -f               # watch auth + relay logs
sudo journalctl -u cloudflared -f
```

## Automated deploys (GitHub Actions)

`.github/workflows/deploy.yml` builds a static musl binary and ships it to your box
over SSH, then restarts the service. It runs on **every push to `master`** (that is
the "auto-update from GitHub") and can also be run by hand (Actions -> Deploy -> Run
workflow).

### Which secrets, and how to get them

Four repo secrets (Settings -> Secrets and variables -> Actions -> New secret):

| Secret | What it is | How to get it |
| --- | --- | --- |
| `SSH_HOST` | your VPS's public IP or hostname | from your VPS provider (e.g. `203.0.113.10`) |
| `SSH_USER` | the deploy SSH user on the VPS | the account you gave to `setup.sh` |
| `SSH_KEY`  | that user's **private** SSH key (whole file) | generated below |
| `SSH_PORT` | SSH port (skip if `22`) | your VPS SSH port |

Generate a dedicated CI key **on your own machine** (no passphrase so CI can use it):

```bash
ssh-keygen -t ed25519 -f ~/.ssh/sechat_ci -N "" -C "sechat-ci"
```

Install the **public** half on the VPS deploy user, then paste the **private** half
into the `SSH_KEY` secret:

```bash
# add the public key to the VPS (lets CI log in):
ssh-copy-id -i ~/.ssh/sechat_ci.pub <deploy-user>@<vps-host>

# copy the PRIVATE key text into the GitHub secret SSH_KEY (the entire file):
cat ~/.ssh/sechat_ci          # -----BEGIN OPENSSH PRIVATE KEY----- ... -----END-----
```

That's it: push to `master` and the box updates itself. The workflow only replaces
the binary and restarts — runtime config (`SECHAT_DEV_INSECURE`, `TLS_*`, …) stays in
`/etc/seserver.env` on the box.

## Persisted state

The server writes its mailbox (offline blobs) to `./mailbox` in its working dir
(`/var/lib/seserver/mailbox`). It is `.gitignore`d; back it up if offline delivery
matters. It only ever holds ciphertext addressed to identity hashes.
