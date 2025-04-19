if [[ -z "${bash_preexec_imported:-}" ]]; then
    echo "bash_preexec is required!" >&2
    echo "see https://github.com/rcaloras/bash-preexec" >&2
elif [[ -n "$__VELLUM_SETUP" ]]; then
    true
else
    readonly __VELLUM_SETUP=1

    VELLUM_SESSION="$(vellum init session)"
    export VELLUM_SESSION

    __vellum_preexec() {
        vellum store -- "$1"
        __VELLUM_LINE=""
    }
    preexec_functions+=(__vellum_preexec)

    __vellum_previous() {
        local vellum_output
        vellum_output="$(vellum move --with-id --session -- -1 "${__VELLUM_LINE}")"
        __VELLUM_LINE="${vellum_output%%|*}"
        READLINE_LINE="${vellum_output#*|}"
        READLINE_POINT="${#READLINE_LINE}"
    }

    __vellum_next() {
        local vellum_output
        vellum_output="$(vellum move --with-id --session -- 1 "${__VELLUM_LINE}")"
        __VELLUM_LINE="${vellum_output%%|*}"
        READLINE_LINE="${vellum_output#*|}"
        READLINE_POINT="${#READLINE_LINE}"
    }

    bind -m emacs -x '"\e[A": __vellum_previous'
    bind -m emacs -x '"\eOA": __vellum_previous'
    bind -m emacs -x '"\e[B": __vellum_next'
    bind -m emacs -x '"\eOB": __vellum_next'
fi
