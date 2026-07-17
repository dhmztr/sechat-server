#!/usr/bin/env bash
# One-time server bootstrap for the sechat relay. Run as root on the VPS from a
# checkout of this repo:
#
#     sudo bash deploy/setup.sh <deploy-ssh-user>
#
# <deploy-ssh-user> is the account GitHub Actions will SSH in as to push new
# binaries. It gets passwordless sudo for ONLY the install + restart commands.
set -euo pipefail

DEPLOY_USER="${1:?usage: sudo bash deploy/setup.sh <deploy-ssh-user>}"
HERE="$(cd "$(dirname "$0")" && pwd)"

if [ "$(id -u)" -ne 0 ]; then
  echo "run as root (sudo)" >&2
  exit 1
fi
if ! id "$DEPLOY_USER" &>/dev/null; then
  echo "deploy user '$DEPLOY_USER' does not exist — create it / pick an existing one" >&2
  exit 1
fi

# service account + state dir (mailbox lives here)
id seserver &>/dev/null || useradd --system --home /var/lib/seserver --create-home seserver
install -d -o seserver -g seserver -m750 /var/lib/seserver

# config (leave existing untouched)
if [ ! -f /etc/seserver.env ]; then
  install -m640 "$HERE/seserver.env.example" /etc/seserver.env
  echo "wrote /etc/seserver.env (edit to switch tunnel <-> TLS)"
fi

# systemd unit
install -m644 "$HERE/seserver.service" /etc/systemd/system/seserver.service

# passwordless sudo for the CI deploy user — install binary + restart ONLY
cat >/etc/sudoers.d/seserver <<EOF
$DEPLOY_USER ALL=(root) NOPASSWD: /usr/bin/install -m755 /tmp/seserver-upload/seserver /usr/local/bin/seserver, /usr/bin/systemctl restart seserver
EOF
chmod 440 /etc/sudoers.d/seserver
visudo -cf /etc/sudoers.d/seserver >/dev/null

systemctl daemon-reload
systemctl enable seserver

echo
echo "Bootstrap done. Next:"
echo "  1. Add the CI deploy key's PUBLIC half to ~$DEPLOY_USER/.ssh/authorized_keys"
echo "  2. Set repo secrets SSH_HOST / SSH_USER=$DEPLOY_USER / SSH_KEY (private) [/ SSH_PORT]"
echo "  3. Push to master (or run the Deploy workflow) — it installs the binary + starts the service."
echo "     (The service won't start until /usr/local/bin/seserver exists, i.e. after the first deploy.)"
