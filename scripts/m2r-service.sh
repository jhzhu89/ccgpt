#!/usr/bin/env bash
set -euo pipefail

ACTION="${1:-}"

SERVICE_NAME="m2r"
UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
UNIT_PATH="${UNIT_DIR}/${SERVICE_NAME}.service"

install_service() {
  local m2r_exec="${M2R_EXEC:-}"
  if [[ -z "${m2r_exec}" ]]; then
    m2r_exec="$(command -v m2r || true)"
  fi

  if [[ -z "${m2r_exec}" ]]; then
    echo "Error: m2r not found in PATH. Set M2R_EXEC or install m2r." >&2
    exit 1
  fi

  local run_user="${RUN_USER:-$(id -un)}"
  local exec_start="${m2r_exec}"

  if [[ -L "${m2r_exec}" ]]; then
    local resolved_exec
    resolved_exec="$(readlink -f "${m2r_exec}")"
    if [[ "${resolved_exec}" == *.js ]]; then
      local bun_exec="${BUN_EXEC:-}"
      if [[ -z "${bun_exec}" ]]; then
        bun_exec="$(command -v bun || true)"
      fi
      if [[ -z "${bun_exec}" ]]; then
        echo "Error: bun not found in PATH. Set BUN_EXEC or install bun." >&2
        exit 1
      fi
      exec_start="${bun_exec} ${resolved_exec}"
    fi
  fi
  local workdir_line=""
  if [[ -n "${M2R_WORKDIR:-}" ]]; then
    workdir_line="WorkingDirectory=${M2R_WORKDIR}"
  fi

  local unit_content="[Unit]
Description=M2r Service
After=network.target

[Service]
Type=simple
${workdir_line}
ExecStart=${exec_start}
Restart=always
Environment=NODE_ENV=production
Environment=PATH=/home/${run_user}/.bun/bin:/usr/local/bin:/usr/bin:/bin

[Install]
WantedBy=multi-user.target
"

  mkdir -p "${UNIT_DIR}"
  printf '%s' "${unit_content}" > "${UNIT_PATH}"
  systemctl --user daemon-reload
  systemctl --user enable "${SERVICE_NAME}"
  systemctl --user restart "${SERVICE_NAME}"

  echo "Installed and started ${SERVICE_NAME}."
}

uninstall_service() {
  systemctl --user stop "${SERVICE_NAME}" || true
  systemctl --user disable "${SERVICE_NAME}" || true
  rm -f "${UNIT_PATH}"
  systemctl --user daemon-reload
  systemctl --user reset-failed "${SERVICE_NAME}" || true

  echo "Uninstalled ${SERVICE_NAME}."
}

usage() {
  echo "Usage: $0 {install|uninstall}"
}

case "${ACTION}" in
  install)
    install_service
    ;;
  uninstall)
    uninstall_service
    ;;
  *)
    usage
    exit 1
    ;;
esac
