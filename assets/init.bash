if [[ -z "${bash_preexec_imported:-}" ]]; then
    echo "bash_preexec is required!" >&2
    echo "see https://github.com/rcaloras/bash-preexec" >&2
elif [[ "$(type -t __fzfcmd)" != "function" ]]; then
    echo "fzf is required!" >&2
    echo "see https://github.com/junegunn/fzf" >&2
elif [[ -n "$__VELLUM_SETUP" || ! $- =~ i ]]; then
    true
else
    readonly __VELLUM_SETUP=1

    VELLUM_SESSION="$(vellum init session)"
    VELLUM_SESSION_START="$(vellum init timestamp)"
    export VELLUM_SESSION VELLUM_SESSION_START

    __vellum_preexec() {
        vellum store -- "$1"
    }
    preexec_functions+=(__vellum_preexec)

    __vellum_precmd() {
        __VELLUM_LINE=""
    }
    precmd_functions+=(__vellum_precmd)

    __vellum_search() {
        local output
        output="$(
            vellum history --fzf "${VELLUM_HISTORY_ARGS[@]}" | \
                FZF_DEFAULT_OPTS=$(__fzf_defaults "" "-n2..,.. --scheme=history --bind=ctrl-r:toggle-sort --wrap-sign '"$'\t'"↳ ' --highlight-line ${FZF_CTRL_R_OPTS-} +m --read0") \
                FZF_DEFAULTS_OPTS_FILE='' $(__fzfcmd) --query "${READLINE_LINE}"
        )" || return
        READLINE_LINE=${output#*$'\t'}
        if [[ -z "$READLINE_POINT" ]]; then
            echo "$READLINE_LINE"
        else
            READLINE_POINT=0x7fffffff
        fi
    }

    __vellum_previous() {
        local vellum_output
        local -a vellum_search
        if [[ -z "${__VELLUM_LINE}" ]]; then
            vellum_search+=("--prefix=${READLINE_LINE}")
        fi
        vellum_output="$(vellum move --with-id --session "${vellum_search[@]}" "${VELLUM_MOVE_ARGS[@]}" -- -1 "${__VELLUM_LINE}")"
        __VELLUM_LINE="${vellum_output%%|*}"
        READLINE_LINE="${vellum_output#*|}"
        READLINE_POINT="${#READLINE_LINE}"
    }

    __vellum_next() {
        local vellum_output
        local -a vellum_search
        if [[ -z "${__VELLUM_LINE}" ]]; then
            vellum_search+=("--prefix=${READLINE_LINE}")
        fi
        vellum_output="$(vellum move --with-id --session "${vellum_search[@]}" "${VELLUM_MOVE_ARGS[@]}" -- 1 "${__VELLUM_LINE}")"
        __VELLUM_LINE="${vellum_output%%|*}"
        READLINE_LINE="${vellum_output#*|}"
        READLINE_POINT="${#READLINE_LINE}"
    }

    bind -m emacs -x '"\C-r": __vellum_search'
    bind -m emacs -x '"\e[A": __vellum_previous'
    bind -m emacs -x '"\eOA": __vellum_previous'
    bind -m emacs -x '"\e[B": __vellum_next'
    bind -m emacs -x '"\eOB": __vellum_next'
fi
