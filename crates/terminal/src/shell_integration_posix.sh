case $- in
  *i*) ;;
  *) return 0 ;;
esac

[ -n "${_OMNIHUB_SHELL_INTEGRATED:-}" ] && return 0
_OMNIHUB_SHELL_INTEGRATED=1
export _OMNIHUB_SHELL_INTEGRATED

__OMNIHUB_HOSTNAME=${HOSTNAME:-$(hostname 2>/dev/null)}
__OMNIHUB_ORIG_PS1=${PS1:-'$ '}

__omnihub_emit_osc() {
  printf '\033]%s\007' "$1"
}

__omnihub_write_cwd_file() {
  [ -n "${OMNIHUB_CWD_FILE:-}" ] || return 0
  printf '%s\n' "$PWD" > "${OMNIHUB_CWD_FILE}" 2>/dev/null || true
}

__omnihub_prompt_prefix() {
  exit_code=$?
  __omnihub_write_cwd_file
  __omnihub_emit_osc "133;D;$exit_code"
  __omnihub_emit_osc "7;file://${__OMNIHUB_HOSTNAME}${PWD}"
  __omnihub_emit_osc '133;A'
}

__omnihub_prompt_suffix() {
  __omnihub_emit_osc '133;B'
}

__omnihub_write_cwd_file
PS1='$(__omnihub_prompt_prefix)'"${__OMNIHUB_ORIG_PS1}"'$(__omnihub_prompt_suffix)'
export PS1
