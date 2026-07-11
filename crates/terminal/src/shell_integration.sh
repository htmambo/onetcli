# 仅在交互式 shell 中生效，避免污染 rsync/scp 等非交互通道
[[ $- != *i* ]] && return
[[ -n "${_OMNIHUB_SHELL_INTEGRATED:-}" ]] && return
export _OMNIHUB_SHELL_INTEGRATED=1

__omnihub_emit_osc() {
    printf '\033]%s\007' "$1"
}

__omnihub_prompt_start() {
    __omnihub_emit_osc '133;A'
}

__omnihub_prompt_end() {
    __omnihub_emit_osc '133;B'
}

__omnihub_command_start() {
    __omnihub_emit_osc '133;C'
}

__omnihub_command_done() {
    __omnihub_emit_osc "133;D;$1"
}

__omnihub_update_cwd() {
    __omnihub_emit_osc "7;file://${HOSTNAME:-$(hostname)}$PWD"
}

__omnihub_write_cwd_file() {
    [[ -n "${OMNIHUB_CWD_FILE:-}" ]] || return 0
    printf '%s\n' "$PWD" > "${OMNIHUB_CWD_FILE}" 2>/dev/null || true
}

# 脚本被 source 时立即写一次初始 cwd
if [[ -n "${OMNIHUB_CWD_FILE:-}" ]]; then
    printf '%s\n' "$PWD" > "${OMNIHUB_CWD_FILE}"
fi

__omnihub_encode_command() {
    command -v base64 >/dev/null 2>&1 || return 1
    printf '%s' "$1" | base64 | tr -d '\r\n'
}

__omnihub_last_history_command() {
    if [[ -n "${ZSH_VERSION:-}" ]]; then
        fc -ln -1 2>/dev/null | sed 's/^[[:space:]]*//'
    else
        history 1 2>/dev/null | sed 's/^[[:space:]]*[0-9][0-9]*[* ]*[[:space:]]*//'
    fi
}

__omnihub_emit_recorded_command() {
    local command_text encoded
    command_text="$(__omnihub_last_history_command)"
    [[ -z "$command_text" ]] && return
    [[ "$command_text" == "${__OMNIHUB_LAST_EMITTED:-}" ]] && return

    encoded="$(__omnihub_encode_command "$command_text")" || return 0
    __OMNIHUB_LAST_EMITTED="$command_text"
    __omnihub_emit_osc "1337;Command=${encoded}"
}

__omnihub_precmd_common() {
    local exit_code="$1"
    __omnihub_command_done "$exit_code"
    if [[ -n "${__OMNIHUB_COMMAND_STARTED:-}" ]]; then
        __omnihub_emit_recorded_command
        unset __OMNIHUB_COMMAND_STARTED
    fi
    __omnihub_write_cwd_file
    __omnihub_update_cwd
    __omnihub_prompt_start
}

if [[ -n "${ZSH_VERSION:-}" ]]; then
    __omnihub_precmd_zsh() {
        __omnihub_precmd_common "$?"
    }

    __omnihub_preexec_zsh() {
        __OMNIHUB_COMMAND_STARTED=1
        __omnihub_command_start
    }

    precmd_functions+=(__omnihub_precmd_zsh)
    preexec_functions+=(__omnihub_preexec_zsh)
    PROMPT="${PROMPT}"$'%{\033]133;B\007%}'
else
    __omnihub_precmd_bash() {
        local exit_code="$?"
        __OMNIHUB_IN_PRECMD=1
        __omnihub_precmd_common "$exit_code"
        __OMNIHUB_IN_PRECMD=0
    }

    __omnihub_preexec_bash() {
        [[ "${__OMNIHUB_IN_PRECMD:-0}" == "1" ]] && return
        [[ "${BASH_COMMAND:-}" == __omnihub_* ]] && return
        __OMNIHUB_COMMAND_STARTED=1
        __omnihub_command_start
    }

    if [[ -z "${PROMPT_COMMAND:-}" ]]; then
        PROMPT_COMMAND='__omnihub_precmd_bash'
    else
        PROMPT_COMMAND="__omnihub_precmd_bash;${PROMPT_COMMAND}"
    fi

    PS1="${PS1}"$'\\[\033]133;B\007\\]'
    trap '__omnihub_preexec_bash' DEBUG
fi
