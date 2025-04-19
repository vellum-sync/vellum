[ -n "$__VELLUM_SETUP" ] && return || readonly __VELLUM_SETUP=1

VELLUM_SESSION="$(vellum init session)"
export VELLUM_SESSION

__vellum_previous() {
    local vellum_output
    vellum_output="$(vellum move --with-id --session -- -1 "${__VELLUM_LINE}")"
    echo "${vellum_output}"
    __VELLUM_LINE="${vellum_output%%|*}"
    READLINE_LINE="${vellum_output#*|}"
    READLINE_POINT="${#READLINE_LINE}"
}

__vellum_next() {
    local vellum_output
    vellum_output="$(vellum move --with-id --session -- 1 "${__VELLUM_LINE}")"
    echo "${vellum_output}"
    __VELLUM_LINE="${vellum_output%%|*}"
    READLINE_LINE="${vellum_output#*|}"
    READLINE_POINT="${#READLINE_LINE}"
}

bind -m emacs -x '"\e[A": __vellum_previous'
bind -m emacs -x '"\eOA": __vellum_previous'
bind -m emacs -x '"\e[B": __vellum_next'
bind -m emacs -x '"\eOB": __vellum_next'
