#!/usr/bin/env bash
# Put the relay behind a Cloudflare Tunnel, so it is reachable at
# wss://<hostname> with Cloudflare's edge TLS — no origin certs, no open ports.
#
#     sudo bash deploy/tunnel.sh relay.example.com [tunnel-name]
#
# One interactive step: `cloudflared tunnel login` prints a URL to open in your
# browser and authorize the domain. Everything else is automatic.
#
# NOTE: a tunnel proxies HTTP/WebSocket only — UDP (STUN + direct P2P) will not
# traverse it, so the app runs relay-only. That is fine; the client falls back to
# the relay automatically.
set -euo pipefail

HOSTNAME="${1:?usage: sudo bash deploy/tunnel.sh <hostname> [tunnel-name]}"
TUNNEL="${2:-sechat}"

if [ "$(id -u)" -ne 0 ]; then
  echo "run as root (sudo)" >&2
  exit 1
fi

# 1. install cloudflared (.deb) if missing
if ! command -v cloudflared >/dev/null 2>&1; then
  arch="$(dpkg --print-architecture)"
  curl -fsSL "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-${arch}.deb" -o /tmp/cloudflared.deb
  dpkg -i /tmp/cloudflared.deb
fi

# 2. authorize the account/domain (writes ~/.cloudflared/cert.pem) — interactive
if [ ! -f /root/.cloudflared/cert.pem ]; then
  echo ">> Opening Cloudflare login. Authorize your domain in the browser."
  cloudflared tunnel login
fi

# 3. create the named tunnel if it does not exist yet
if ! cloudflared tunnel list | awk '{print $2}' | grep -qx "$TUNNEL"; then
  cloudflared tunnel create "$TUNNEL"
fi
UUID="$(cloudflared tunnel list | awk -v n="$TUNNEL" '$2==n{print $1; exit}')"
[ -n "${UUID:-}" ] || { echo "could not determine tunnel UUID" >&2; exit 1; }

# 4. route the hostname to this tunnel (creates the DNS record)
cloudflared tunnel route dns "$TUNNEL" "$HOSTNAME" || true

# 5. install config + credentials under /etc/cloudflared (read by the root service)
install -d /etc/cloudflared
install -m600 "/root/.cloudflared/${UUID}.json" "/etc/cloudflared/${UUID}.json"
cat >/etc/cloudflared/config.yml <<EOF
tunnel: $TUNNEL
credentials-file: /etc/cloudflared/${UUID}.json

ingress:
  - hostname: $HOSTNAME
    service: http://localhost:3000
  - service: http_status:404
EOF

# 6. run it as a systemd service
cloudflared service install || true
systemctl enable cloudflared
systemctl restart cloudflared

echo
echo "Tunnel up: https://$HOSTNAME -> http://localhost:3000"
echo "Point clients at:  SECHAT_SERVER=$HOSTNAME   (client uses wss:// by default)"
echo "Check:  systemctl status cloudflared ;  curl -sI https://$HOSTNAME/ws"
