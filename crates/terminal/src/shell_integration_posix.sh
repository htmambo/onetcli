case $- in
  *i*) ;;
  *) return 0 ;;
esac

[ -n "${_ONETCLI_SHELL_INTEGRATED:-}" ] && return 0
_ONETCLI_SHELL_INTEGRATED=1
export _ONETCLI_SHELL_INTEGRATED

__ONETCLI_HOSTNAME=${HOSTNAME:-$(hostname 2>/dev/null)}
__ONETCLI_ORIG_PS1=${PS1:-'$ '}

__onetcli_emit_osc() {
  printf '\033]%s\007' "$1"
}

__onetcli_write_cwd_file() {
  [ -n "${ONETCLI_CWD_FILE:-}" ] || return 0
  printf '%s\n' "$PWD" > "${ONETCLI_CWD_FILE}" 2>/dev/null || true
}

__onetcli_prompt_prefix() {
  exit_code=$?
  __onetcli_write_cwd_file
  __onetcli_emit_osc "133;D;$exit_code"
  __onetcli_emit_osc "7;file://${__ONETCLI_HOSTNAME}${PWD}"
  __onetcli_emit_osc '133;A'
}

__onetcli_prompt_suffix() {
  __onetcli_emit_osc '133;B'
}

__onetcli_write_cwd_file
PS1='$(__onetcli_prompt_prefix)'"${__ONETCLI_ORIG_PS1}"'$(__onetcli_prompt_suffix)'
export PS1
